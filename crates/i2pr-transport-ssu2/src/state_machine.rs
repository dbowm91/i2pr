//! Consuming SSU2 v2 initiator/responder establishment state machines.
//!
//! The machines own protocol sequencing only. A runtime (Plan 158)
//! fulfills the returned actions over UDP and supplies results back
//! through consuming transitions. No action sleeps, opens sockets,
//! spawns tasks, reads randomness, or consults wall-clock time: all
//! secrets, connection IDs, packet numbers, timestamps, and clock
//! readings arrive as explicit parameters, so every trajectory is
//! deterministic under test.
//!
//! Retransmission follows the specification schedules with bounded
//! attempt counts and a central handshake deadline; no timer object is
//! created per datagram. The protocol crate exposes deadline values;
//! the runtime owns the actual timers.
//!
//! Normative traceability: SSU2 specification sections Session
//! Establishment, Packet Numbering (handshake resend rules), and the
//! handshake KDF/message sections referenced from `handshake.rs`.

use std::{net::SocketAddr, vec::Vec};

use i2pr_crypto::X25519PrivateKey;
use i2pr_proto::Hash;
use thiserror::Error;

use crate::block::{
    AddressBlock, Block as PayloadBlock, PaddingBlock as Pad, TimestampBlock as Ts, encode_blocks,
};
use crate::constants;
use crate::crypto::{
    IntroKey, Role, Ssu2CryptoError, Ssu2PublicKey, Ssu2SplitKeys, Ssu2Transcript,
    session_confirmed_header_key, session_created_header_key,
};
use crate::handshake::{
    AuthenticatedPeer, ClockSkewPolicy, ConfirmedReassembly, HandshakeError, HandshakeReplayCache,
    ReplayDecision, ReplayToken, RouterInfoFreshness, build_confirmed_payload, build_retry,
    build_session_confirmed, build_session_created, build_session_request, build_token_request,
    parse_retry, parse_session_created, parse_session_request, parse_token_request,
    require_first_router_info, require_timestamp, split_confirmed_jumbo, validate_router_info,
};
use crate::header::{LongHeader, MessageType, SessionConfirmedHeader};
use crate::token::TokenStore;

/// Typed failures from establishment state-machine sequencing.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum StateMachineError {
    /// A handshake-layer operation failed.
    #[error("SSU2 handshake failed")]
    Handshake(#[from] HandshakeError),
    /// A transcript operation failed.
    #[error("SSU2 transcript failed")]
    Crypto(#[from] Ssu2CryptoError),
    /// A protocol-crypto wrapper rejected its input.
    #[error("SSU2 protocol crypto wrapper rejected input")]
    Wrapper(#[from] i2pr_crypto::CryptoError),
    /// The machine was driven in a state that forbids the input.
    #[error("SSU2 establishment input is invalid in this state")]
    InvalidState,
}

impl From<crate::token::TokenError> for StateMachineError {
    fn from(value: crate::token::TokenError) -> Self {
        Self::Handshake(value.into())
    }
}

/// Bounded owned handshake bytes with redacted diagnostics.
pub struct DatagramBytes(Vec<u8>);

impl DatagramBytes {
    fn new(bytes: Vec<u8>) -> Result<Self, StateMachineError> {
        if bytes.len() > constants::MAX_DATAGRAM_IPV4_LENGTH {
            return Err(StateMachineError::Handshake(HandshakeError::TooLong));
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

impl std::fmt::Debug for DatagramBytes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DatagramBytes")
            .field("length", &self.len())
            .finish()
    }
}

/// Which deadline a returned action arms at the runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeadlineKind {
    /// TokenRequest resend / timeout.
    TokenRequest,
    /// SessionRequest resend / timeout.
    SessionRequest,
    /// SessionCreated resend / timeout.
    SessionCreated,
    /// SessionConfirmed resend / timeout.
    SessionConfirmed,
    /// Terminal handshake deadline.
    Handshake,
}

/// Why establishment terminated without a session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminateReason {
    /// The handshake deadline elapsed.
    HandshakeTimeout,
    /// Retransmission attempts were exhausted.
    RetriesExhausted,
    /// The operation was cancelled by the owner.
    Cancelled,
    /// Authentication (AEAD/DH/transcript) failed.
    AuthenticationFailed,
    /// The peer's RouterInfo failed establishment validation.
    RouterInfoRejected,
    /// A token was missing, unknown, expired, reused, or misbound.
    TokenRejected,
    /// A replayed establishment value was observed.
    ReplayDetected,
    /// The peer rejected the handshake with a termination.
    PeerTerminated,
    /// A datagram violated protocol shape beyond silent-drop scope.
    ProtocolViolation,
}

/// Why an inbound datagram was silently dropped. Categories are local
/// diagnostics only and are never reflected to unauthenticated sources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropCategory {
    /// The datagram failed length or structural checks.
    Malformed,
    /// Version, network ID, or type checks failed.
    VersionNetworkType,
    /// Source and destination connection IDs matched.
    ConnectionIdMismatch,
    /// A required token was absent or invalid.
    BadToken,
    /// A replayed value was observed.
    Replay,
    /// A timestamp fell outside the skew window.
    ClockSkew,
    /// A Retry carried a peer termination (no further request desired).
    PeerTerminated,
    /// The datagram belongs to an unknown or completed handshake.
    Unexpected,
}

/// An immediate runtime operation emitted by an establishment state.
pub enum HandshakeAction {
    /// Emit one owned, bounded datagram.
    WriteDatagram(DatagramBytes),
    /// Arm one deadline at the runtime (milliseconds, caller clock).
    ArmDeadline {
        /// Which deadline to arm.
        kind: DeadlineKind,
        /// Caller-clock time at which the deadline fires.
        at_ms: u64,
    },
    /// Establishment completed with authenticated session material.
    Established(AuthenticatedSsu2Session),
    /// Establishment terminated; release all state.
    Terminate(TerminateReason),
    /// The inbound datagram was dropped without a response.
    DropSilently(DropCategory),
}

impl std::fmt::Debug for HandshakeAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WriteDatagram(bytes) => {
                formatter.debug_tuple("WriteDatagram").field(bytes).finish()
            }
            Self::ArmDeadline { kind, at_ms } => formatter
                .debug_struct("ArmDeadline")
                .field("kind", kind)
                .field("at_ms", at_ms)
                .finish(),
            Self::Established(_) => formatter.write_str("Established(<redacted>)"),
            Self::Terminate(reason) => formatter.debug_tuple("Terminate").field(reason).finish(),
            Self::DropSilently(category) => formatter
                .debug_tuple("DropSilently")
                .field(category)
                .finish(),
        }
    }
}

/// The narrow post-handshake output Plan 157 needs. Intermediate Noise
/// secrets are released when the transcript splits; only directional
/// data-phase ciphers remain.
pub struct AuthenticatedSsu2Session {
    peer: AuthenticatedPeer,
    keys: Ssu2SplitKeys,
    local_conn_id: u64,
    remote_conn_id: u64,
    peer_endpoint: SocketAddr,
    local_mtu: u16,
}

impl AuthenticatedSsu2Session {
    /// Returns the validated authenticated peer material.
    pub const fn peer(&self) -> &AuthenticatedPeer {
        &self.peer
    }

    /// Borrows the directional data-phase ciphers.
    pub const fn keys(&mut self) -> &mut Ssu2SplitKeys {
        &mut self.keys
    }

    /// Returns the locally allocated connection ID.
    pub const fn local_conn_id(&self) -> u64 {
        self.local_conn_id
    }

    /// Returns the peer's connection ID.
    pub const fn remote_conn_id(&self) -> u64 {
        self.remote_conn_id
    }

    /// Returns the observed peer endpoint (source of the handshake).
    pub const fn peer_endpoint(&self) -> SocketAddr {
        self.peer_endpoint
    }

    /// Returns the local MTU constraint.
    pub const fn local_mtu(&self) -> u16 {
        self.local_mtu
    }
}

/// Initiator dial context: responder material from the dial target plus
/// local policy. Noise XK requires the responder static key in advance,
/// so the dial context always names the expected peer hash as well. No
/// sockets or clocks are captured.
pub struct InitiatorConfig {
    /// Responder static key `s` (Noise pre-message).
    pub responder_static: Ssu2PublicKey,
    /// Responder intro key `i` (header protection + Retry AEAD).
    pub responder_intro: IntroKey,
    /// Expected responder RouterIdentity hash from the dial context.
    pub expected_router_hash: Hash,
    /// Handshake timestamp skew policy.
    pub clock: ClockSkewPolicy,
    /// Local MTU constraint recorded into the established session.
    pub local_mtu: u16,
}

/// Caller-supplied secrets and wire values for one initiator attempt.
pub struct InitiatorSecrets {
    /// Initiator static secret (for `es`/`se`; retained across Retry).
    pub static_secret: X25519PrivateKey,
    /// Fresh ephemeral secret for this SessionRequest.
    pub ephemeral_secret: X25519PrivateKey,
    /// Local connection ID (constant across Retry for this handshake).
    pub local_conn_id: u64,
    /// Destination connection ID for the request (random, opaque).
    pub remote_conn_id: u64,
    /// Request packet number (random, ignored by the peer).
    pub packet_number: u32,
    /// Request payload timestamp (Unix seconds).
    pub timestamp: u32,
}

/// Fresh wire values for the request that answers a Retry.
pub struct RetryAnswer {
    /// Fresh ephemeral secret for the token-bearing SessionRequest.
    pub ephemeral_secret: X25519PrivateKey,
    /// Fresh request packet number (random, ignored by the peer).
    pub packet_number: u32,
    /// Fresh request payload timestamp (Unix seconds).
    pub timestamp: u32,
    /// Padding bytes for the request payload.
    pub padding: Vec<u8>,
}

/// Parameters for emitting SessionConfirmed after SessionCreated.
pub struct ConfirmedParams {
    /// Complete local RouterInfo bytes for the first payload block.
    pub router_info: Vec<u8>,
    /// Padding bytes for the confirmed payload.
    pub padding: Vec<u8>,
    /// Fragment payload budget per datagram.
    pub mtu_payload: usize,
    /// Observed responder endpoint recorded into the session.
    pub peer_endpoint: SocketAddr,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum InitiatorState {
    AwaitRetryOrCreated,
    Established,
}

/// A consuming SSU2 initiator: TokenRequest/SessionRequest, Retry
/// handling, SessionCreated acceptance, and SessionConfirmed emission.
pub struct Initiator {
    config: InitiatorConfig,
    transcript: Option<Ssu2Transcript>,
    static_secret: Option<X25519PrivateKey>,
    ephemeral_secret: Option<X25519PrivateKey>,
    request_ciphertext: Vec<u8>,
    created_chain_key: Option<[u8; 32]>,
    confirmed_chain_key: Option<[u8; 32]>,
    responder_eph: Option<Ssu2PublicKey>,
    token: Option<u64>,
    local_conn_id: u64,
    remote_conn_id: u64,
    last_datagram: Vec<u8>,
    confirmed_datagrams: Vec<Vec<u8>>,
    deadline_kind: DeadlineKind,
    resend_index: usize,
    resend_delays: &'static [u64],
    attempts: u8,
    started_at_ms: u64,
    deadline_at_ms: u64,
    state: InitiatorState,
}

impl Initiator {
    /// Begins establishment: emits TokenRequest when no token is held,
    /// otherwise emits SessionRequest. All randomness arrives via
    /// `secrets`/`padding`; `now_ms` anchors the schedules.
    pub fn begin(
        config: InitiatorConfig,
        secrets: InitiatorSecrets,
        token: Option<u64>,
        payload_padding: Vec<u8>,
        now_ms: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        let mut initiator = Self {
            config,
            transcript: None,
            static_secret: Some(secrets.static_secret),
            ephemeral_secret: None,
            request_ciphertext: Vec::new(),
            created_chain_key: None,
            confirmed_chain_key: None,
            responder_eph: None,
            token,
            local_conn_id: secrets.local_conn_id,
            remote_conn_id: secrets.remote_conn_id,
            last_datagram: Vec::new(),
            confirmed_datagrams: Vec::new(),
            deadline_kind: DeadlineKind::SessionRequest,
            resend_index: 0,
            resend_delays: &constants::SESSION_REQUEST_RESEND_DELAYS_MS,
            attempts: 0,
            started_at_ms: now_ms,
            deadline_at_ms: now_ms.saturating_add(constants::HANDSHAKE_DEADLINE_MS),
            state: InitiatorState::AwaitRetryOrCreated,
        };
        let actions = if token.is_some() {
            initiator.seal_request(
                secrets.ephemeral_secret,
                secrets.packet_number,
                secrets.timestamp,
                payload_padding,
            )?
        } else {
            initiator.send_token_request(
                secrets.packet_number,
                secrets.timestamp,
                payload_padding,
            )?
        };
        Ok((initiator, actions))
    }

    fn arm_handshake_deadline(&self) -> HandshakeAction {
        HandshakeAction::ArmDeadline {
            kind: DeadlineKind::Handshake,
            at_ms: self.deadline_at_ms,
        }
    }

    fn send_token_request(
        &mut self,
        packet_number: u32,
        timestamp: u32,
        padding: Vec<u8>,
    ) -> Result<Vec<HandshakeAction>, StateMachineError> {
        let datagram = build_token_request(
            &self.config.responder_intro,
            self.local_conn_id,
            self.remote_conn_id,
            packet_number,
            timestamp,
            padding,
        )?;
        self.last_datagram = datagram.clone();
        self.attempts = 1;
        self.resend_index = 0;
        self.deadline_kind = DeadlineKind::TokenRequest;
        self.resend_delays = &constants::TOKEN_REQUEST_RESEND_DELAYS_MS;
        self.state = InitiatorState::AwaitRetryOrCreated;
        Ok(vec![
            HandshakeAction::WriteDatagram(DatagramBytes::new(datagram)?),
            HandshakeAction::ArmDeadline {
                kind: DeadlineKind::TokenRequest,
                at_ms: self.started_at_ms.saturating_add(self.resend_delays[0]),
            },
            self.arm_handshake_deadline(),
        ])
    }

    fn seal_request(
        &mut self,
        ephemeral_secret: X25519PrivateKey,
        packet_number: u32,
        timestamp: u32,
        padding: Vec<u8>,
    ) -> Result<Vec<HandshakeAction>, StateMachineError> {
        let mut blocks = Vec::with_capacity(2);
        blocks.push(PayloadBlock::Timestamp(Ts::new(timestamp)));
        // The DateTime block alone is 7 bytes; the handshake minimum
        // is 8, so an empty caller padding tops up with one zero byte
        // rather than failing the whole handshake.
        let padding = if padding.is_empty() {
            vec![0_u8; 1]
        } else {
            padding
        };
        blocks.push(PayloadBlock::Padding(
            Pad::new(padding).map_err(HandshakeError::Blocks)?,
        ));
        let payload = encode_blocks(blocks).map_err(HandshakeError::Blocks)?;
        let ephemeral_public = Ssu2PublicKey::new(ephemeral_secret.public_bytes())?;
        let es = ephemeral_secret.diffie_hellman(self.config.responder_static.as_bytes())?;
        let header = LongHeader::new(
            self.remote_conn_id,
            packet_number,
            MessageType::SessionRequest,
            self.local_conn_id,
            self.token.unwrap_or(0),
        )
        .map_err(HandshakeError::from)?;
        let transcript = Ssu2Transcript::new(Role::Initiator, self.config.responder_static);
        let (transcript, ciphertext) =
            transcript.seal_session_request(&header.encode(), ephemeral_public, es, &payload)?;
        self.created_chain_key = Some(transcript.evidence_chain_key());
        let datagram = build_session_request(
            &header,
            &ephemeral_public,
            &ciphertext,
            self.config.responder_intro.as_bytes(),
            self.config.responder_intro.as_bytes(),
        )?;
        self.request_ciphertext = ciphertext;
        self.transcript = Some(transcript);
        self.ephemeral_secret = Some(ephemeral_secret);
        self.last_datagram = datagram.clone();
        self.attempts = 1;
        self.resend_index = 0;
        self.deadline_kind = DeadlineKind::SessionRequest;
        self.resend_delays = &constants::SESSION_REQUEST_RESEND_DELAYS_MS;
        self.state = InitiatorState::AwaitRetryOrCreated;
        Ok(vec![
            HandshakeAction::WriteDatagram(DatagramBytes::new(datagram)?),
            HandshakeAction::ArmDeadline {
                kind: DeadlineKind::SessionRequest,
                at_ms: self.started_at_ms.saturating_add(self.resend_delays[0]),
            },
            self.arm_handshake_deadline(),
        ])
    }

    /// Handles an inbound Retry: validates it, records the token, and
    /// emits a token-bearing SessionRequest with fresh caller-supplied
    /// ephemeral material. A peer termination drops silently.
    pub fn on_retry(
        mut self,
        mut datagram: Vec<u8>,
        answer: RetryAnswer,
        now_ms: u64,
        now_secs: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        if self.state == InitiatorState::Established {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::Unexpected)],
            ));
        }
        if now_ms >= self.deadline_at_ms {
            return Ok((
                self,
                vec![HandshakeAction::Terminate(
                    TerminateReason::HandshakeTimeout,
                )],
            ));
        }
        let retry = match parse_retry(
            &mut datagram,
            &self.config.responder_intro,
            self.config.clock,
            now_secs,
        ) {
            Ok(retry) => retry,
            Err(HandshakeError::StaleTimestamp) | Err(HandshakeError::FutureTimestamp) => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::ClockSkew)],
                ));
            }
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                ));
            }
        };
        if retry.termination().is_some() {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::PeerTerminated)],
            ));
        }
        if retry.token() == 0 {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::BadToken)],
            ));
        }
        self.token = Some(retry.token());
        let actions = self.seal_request(
            answer.ephemeral_secret,
            answer.packet_number,
            answer.timestamp,
            answer.padding,
        )?;
        Ok((self, actions))
    }

    /// Handles an inbound SessionCreated: completes the Noise handshake,
    /// emits SessionConfirmed fragments carrying the local RouterInfo,
    /// and reports the established session.
    pub fn on_session_created(
        mut self,
        mut datagram: Vec<u8>,
        confirmed: ConfirmedParams,
        now_ms: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        if self.state == InitiatorState::Established {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::Unexpected)],
            ));
        }
        if now_ms >= self.deadline_at_ms {
            return Ok((
                self,
                vec![HandshakeAction::Terminate(
                    TerminateReason::HandshakeTimeout,
                )],
            ));
        }
        let chain_key = self
            .created_chain_key
            .ok_or(StateMachineError::InvalidState)?;
        let created_header_key = session_created_header_key(&chain_key)?;
        let parts = match parse_session_created(
            &mut datagram,
            self.config.responder_intro.as_bytes(),
            &created_header_key,
        ) {
            Ok(parts) => parts,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                ));
            }
        };
        if parts.header.dst_conn_id() != self.local_conn_id {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(
                    DropCategory::ConnectionIdMismatch,
                )],
            ));
        }
        let transcript = self
            .transcript
            .take()
            .ok_or(StateMachineError::InvalidState)?;
        let ephemeral_secret = self
            .ephemeral_secret
            .take()
            .ok_or(StateMachineError::InvalidState)?;
        let ee = ephemeral_secret.diffie_hellman(parts.ephemeral.as_bytes())?;
        let request_ciphertext = std::mem::take(&mut self.request_ciphertext);
        let (transcript, _created_payload) = match transcript.accept_session_created(
            &request_ciphertext,
            &parts.header.encode(),
            parts.ephemeral,
            ee,
            &parts.ciphertext,
        ) {
            Ok(value) => value,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::Terminate(
                        TerminateReason::AuthenticationFailed,
                    )],
                ));
            }
        };
        self.confirmed_chain_key = Some(transcript.evidence_chain_key());
        self.responder_eph = Some(parts.ephemeral);
        let static_secret = self
            .static_secret
            .take()
            .ok_or(StateMachineError::InvalidState)?;
        let alice_public = Ssu2PublicKey::new(static_secret.public_bytes())?;
        let (transcript, static_frame) = transcript.seal_confirmed_static(alice_public)?;
        let responder_eph = self.responder_eph.ok_or(StateMachineError::InvalidState)?;
        let se = static_secret.diffie_hellman(responder_eph.as_bytes())?;
        let confirmed_payload = build_confirmed_payload(&confirmed.router_info, confirmed.padding)?;
        let (transcript, confirmed_ct) =
            transcript.seal_confirmed_payload(se, &confirmed_payload)?;
        let mut jumbo = Vec::with_capacity(static_frame.len() + confirmed_ct.len());
        jumbo.extend_from_slice(&static_frame);
        jumbo.extend_from_slice(&confirmed_ct);
        let confirmed_key = session_confirmed_header_key(
            &self
                .confirmed_chain_key
                .ok_or(StateMachineError::InvalidState)?,
        )?;
        let fragments = build_session_confirmed(
            parts.header.src_conn_id(),
            &jumbo,
            confirmed.mtu_payload,
            self.config.responder_intro.as_bytes(),
            &confirmed_key,
        )?;
        let keys = transcript.split()?;
        let peer = AuthenticatedPeer {
            router_hash: self.config.expected_router_hash,
            transport_static_key: self.config.responder_static,
            router_info: Vec::new(),
        };
        let session = AuthenticatedSsu2Session {
            peer,
            keys,
            local_conn_id: self.local_conn_id,
            remote_conn_id: parts.header.src_conn_id(),
            peer_endpoint: confirmed.peer_endpoint,
            local_mtu: self.config.local_mtu,
        };
        self.confirmed_datagrams = fragments.clone();
        self.attempts = 1;
        self.resend_index = 0;
        self.deadline_kind = DeadlineKind::SessionConfirmed;
        self.resend_delays = &constants::SESSION_CONFIRMED_RESEND_DELAYS_MS;
        self.state = InitiatorState::Established;
        let mut actions = Vec::with_capacity(fragments.len() + 2);
        for fragment in fragments {
            actions.push(HandshakeAction::WriteDatagram(DatagramBytes::new(
                fragment,
            )?));
        }
        actions.push(HandshakeAction::ArmDeadline {
            kind: DeadlineKind::SessionConfirmed,
            at_ms: now_ms.saturating_add(self.resend_delays[0]),
        });
        actions.push(HandshakeAction::Established(session));
        Ok((self, actions))
    }

    /// Handles an unexpected datagram shape for the current state.
    pub fn on_unexpected(self) -> (Self, Vec<HandshakeAction>) {
        (
            self,
            vec![HandshakeAction::DropSilently(DropCategory::Unexpected)],
        )
    }

    /// Handles a retransmit/timeout firing at `now_ms`: resends the
    /// identical last datagram while attempts remain, else terminates.
    /// After establishment this resends SessionConfirmed fragments
    /// until the runtime reports data-phase activity (Plan 157/158).
    pub fn on_timeout(
        mut self,
        now_ms: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        if now_ms >= self.deadline_at_ms {
            return Ok((
                self,
                vec![HandshakeAction::Terminate(
                    TerminateReason::HandshakeTimeout,
                )],
            ));
        }
        let next_index = self.resend_index + 1;
        if next_index >= self.resend_delays.len()
            || self.attempts >= constants::MAX_HANDSHAKE_ATTEMPTS
        {
            return Ok((
                self,
                vec![HandshakeAction::Terminate(
                    TerminateReason::RetriesExhausted,
                )],
            ));
        }
        self.resend_index = next_index;
        self.attempts += 1;
        let at_ms = now_ms.saturating_add(self.resend_delays[next_index]);
        let deadline_kind = self.deadline_kind;
        if self.state == InitiatorState::Established && !self.confirmed_datagrams.is_empty() {
            let mut actions = Vec::with_capacity(self.confirmed_datagrams.len() + 1);
            for fragment in &self.confirmed_datagrams {
                actions.push(HandshakeAction::WriteDatagram(DatagramBytes::new(
                    fragment.clone(),
                )?));
            }
            actions.push(HandshakeAction::ArmDeadline {
                kind: deadline_kind,
                at_ms,
            });
            return Ok((self, actions));
        }
        let resend = DatagramBytes::new(self.last_datagram.clone())?;
        Ok((
            self,
            vec![
                HandshakeAction::WriteDatagram(resend),
                HandshakeAction::ArmDeadline {
                    kind: deadline_kind,
                    at_ms,
                },
            ],
        ))
    }

    /// Cancels establishment; all retained state is released with the machine.
    pub fn cancel(self) -> HandshakeAction {
        HandshakeAction::Terminate(TerminateReason::Cancelled)
    }
}

/// Responder configuration: local secrets plus policy.
pub struct ResponderConfig {
    /// Responder static secret (for `es`/`se`).
    pub static_secret: X25519PrivateKey,
    /// Responder intro key (header protection + Retry AEAD).
    pub intro_key: IntroKey,
    /// Expected initiator RouterIdentity hash, when known.
    pub expected_peer_hash: Option<Hash>,
    /// Handshake timestamp skew policy.
    pub clock: ClockSkewPolicy,
    /// Local MTU constraint recorded into the established session.
    pub local_mtu: u16,
    /// Responder address block echoed in Retry payloads.
    pub local_address: AddressBlock,
}

/// Per-handshake responder parameters supplied by the driver when a new
/// SessionRequest is admitted.
pub struct ResponderParams {
    /// Responder connection ID for Created/Retry (constant per handshake).
    pub local_conn_id: u64,
    /// Fresh ephemeral secret for SessionCreated.
    pub ephemeral_secret: X25519PrivateKey,
    /// Created/Retry packet number (random, ignored by the peer).
    pub packet_number: u32,
    /// Created/Retry payload timestamp (Unix seconds).
    pub timestamp: u32,
    /// Padding bytes for the Created payload.
    pub padding: Vec<u8>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ResponderState {
    AwaitRequest,
    AwaitConfirmed,
}

/// A consuming SSU2 responder: TokenRequest/Retry, SessionRequest
/// admission (token before DH), SessionCreated emission, and
/// SessionConfirmed reassembly/validation.
pub struct Responder {
    config: ResponderConfig,
    transcript: Option<Ssu2Transcript>,
    ephemeral_secret: Option<X25519PrivateKey>,
    created_chain_key: Option<[u8; 32]>,
    confirmed_chain_key: Option<[u8; 32]>,
    local_conn_id: u64,
    initiator_eph: Option<Ssu2PublicKey>,
    initiator_conn_id: u64,
    created_datagram: Vec<u8>,
    reassembly: Option<ConfirmedReassembly>,
    attempts: u8,
    deadline_at_ms: u64,
    state: ResponderState,
}

impl Responder {
    /// Creates a responder awaiting its first TokenRequest/SessionRequest.
    pub fn new(config: ResponderConfig, now_ms: u64) -> Self {
        Self {
            config,
            transcript: None,
            ephemeral_secret: None,
            created_chain_key: None,
            confirmed_chain_key: None,
            local_conn_id: 0,
            initiator_eph: None,
            initiator_conn_id: 0,
            created_datagram: Vec::new(),
            reassembly: None,
            attempts: 0,
            deadline_at_ms: now_ms.saturating_add(constants::HANDSHAKE_DEADLINE_MS),
            state: ResponderState::AwaitRequest,
        }
    }

    /// Classifies one inbound datagram cheaply: deprotects a working
    /// copy of the first 16 bytes with the intro key (TokenRequest,
    /// SessionRequest, and Retry share `bik` for both halves) and reads
    /// the type byte. SessionConfirmed uses the derived second key and
    /// is routed by handshake state, not here; the driver should offer
    /// unclassifiable datagrams to `on_session_confirmed` while a
    /// handshake awaits confirmation.
    pub fn classify(&self, datagram: &[u8]) -> Option<MessageType> {
        deprotect_type_byte(
            datagram,
            self.config.intro_key.as_bytes(),
            self.config.intro_key.as_bytes(),
        )
        .ok()
    }

    /// Handles one inbound TokenRequest: validates skew and answers
    /// with a Retry carrying a fresh source-bound token. No session
    /// state is allocated and no DH is performed.
    #[allow(clippy::too_many_arguments)]
    pub fn on_token_request(
        self,
        mut datagram: Vec<u8>,
        source: SocketAddr,
        local_conn_id: u64,
        packet_number: u32,
        timestamp: u32,
        padding: Vec<u8>,
        token_bytes: [u8; 8],
        store: &mut TokenStore,
        now_secs: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        if self.state != ResponderState::AwaitRequest {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::Unexpected)],
            ));
        }
        let request = match parse_token_request(
            &mut datagram,
            &self.config.intro_key,
            self.config.clock,
            now_secs,
        ) {
            Ok(request) => request,
            Err(HandshakeError::StaleTimestamp) | Err(HandshakeError::FutureTimestamp) => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::ClockSkew)],
                ));
            }
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                ));
            }
        };
        let token = store
            .issue(source, now_secs, token_bytes)
            .map_err(StateMachineError::from)?;
        let retry = build_retry(
            &self.config.intro_key,
            datagram.len(),
            request.header().src_conn_id(),
            local_conn_id,
            packet_number,
            token.value(),
            timestamp,
            self.config.local_address,
            None,
            padding,
        )?;
        Ok((
            self,
            vec![HandshakeAction::WriteDatagram(DatagramBytes::new(retry)?)],
        ))
    }

    /// Handles one inbound SessionRequest. Tokenless requests earn a
    /// Retry (source-bound token, no DH); token-bearing requests pass
    /// token, replay, and skew gates before the single admitted DH and
    /// SessionCreated emission. Duplicates of the admitted request
    /// resend the identical SessionCreated.
    #[allow(clippy::too_many_arguments)]
    pub fn on_session_request(
        mut self,
        mut datagram: Vec<u8>,
        source: SocketAddr,
        params: ResponderParams,
        retry_token_bytes: [u8; 8],
        store: &mut TokenStore,
        replay: &mut HandshakeReplayCache,
        now_ms: u64,
        now_secs: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        if now_ms >= self.deadline_at_ms {
            return Ok((
                self,
                vec![HandshakeAction::Terminate(
                    TerminateReason::HandshakeTimeout,
                )],
            ));
        }
        if self.state == ResponderState::AwaitConfirmed {
            return Ok(self.answer_request_while_confirming(&mut datagram));
        }
        let request_length = datagram.len();
        let parts = match parse_session_request(&mut datagram, &self.config.intro_key) {
            Ok(parts) => parts,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                ));
            }
        };
        if parts.header.token() == 0 {
            return self.emit_retry_for_request(
                &parts.header,
                source,
                request_length,
                params,
                retry_token_bytes,
                store,
                now_secs,
            );
        }
        if store
            .consume(parts.header.token(), source, now_secs)
            .is_err()
        {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::BadToken)],
            ));
        }
        let replay_token = ReplayToken::from_ephemeral_bytes(parts.ephemeral.as_bytes());
        match replay.check_and_record(replay_token, now_secs) {
            ReplayDecision::Fresh => {}
            ReplayDecision::Replayed => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::Replay)],
                ));
            }
            ReplayDecision::CacheFull => {
                return Ok((
                    self,
                    vec![HandshakeAction::Terminate(TerminateReason::ReplayDetected)],
                ));
            }
        }
        let es = self
            .config
            .static_secret
            .diffie_hellman(parts.ephemeral.as_bytes())
            .map_err(|_| {
                StateMachineError::Handshake(HandshakeError::Crypto(
                    Ssu2CryptoError::AuthenticationFailed,
                ))
            })?;
        let transcript = Ssu2Transcript::new(Role::Responder, self.local_static_public()?);
        let (transcript, payload) = match transcript.accept_session_request(
            &parts.header.encode(),
            parts.ephemeral,
            es,
            &parts.ciphertext,
        ) {
            Ok(value) => value,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                ));
            }
        };
        let timestamp = match require_timestamp(&payload) {
            Ok(timestamp) => timestamp,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                ));
            }
        };
        if self.config.clock.classify(now_secs, timestamp).is_err() {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::ClockSkew)],
            ));
        }
        self.created_chain_key = Some(transcript.evidence_chain_key());
        let responder_eph_public = Ssu2PublicKey::new(params.ephemeral_secret.public_bytes())?;
        let ee = params
            .ephemeral_secret
            .diffie_hellman(parts.ephemeral.as_bytes())
            .map_err(|_| {
                StateMachineError::Handshake(HandshakeError::Crypto(
                    Ssu2CryptoError::AuthenticationFailed,
                ))
            })?;
        let mut blocks = Vec::with_capacity(3);
        blocks.push(PayloadBlock::Timestamp(Ts::new(params.timestamp)));
        blocks.push(PayloadBlock::Address(self.config.local_address));
        if !params.padding.is_empty() {
            blocks.push(PayloadBlock::Padding(
                Pad::new(params.padding).map_err(HandshakeError::Blocks)?,
            ));
        }
        let created_payload = encode_blocks(blocks).map_err(HandshakeError::Blocks)?;
        let created_header = LongHeader::new(
            parts.header.src_conn_id(),
            params.packet_number,
            MessageType::SessionCreated,
            params.local_conn_id,
            0,
        )
        .map_err(HandshakeError::from)?;
        let (transcript, created_ct) = transcript.seal_session_created(
            &parts.ciphertext,
            &created_header.encode(),
            responder_eph_public,
            ee,
            &created_payload,
        )?;
        self.confirmed_chain_key = Some(transcript.evidence_chain_key());
        let created_key = session_created_header_key(
            &self
                .created_chain_key
                .ok_or(StateMachineError::InvalidState)?,
        )?;
        let created_datagram = build_session_created(
            &created_header,
            &responder_eph_public,
            &created_ct,
            self.config.intro_key.as_bytes(),
            &created_key,
        )?;
        self.created_datagram = created_datagram.clone();
        self.transcript = Some(transcript);
        self.ephemeral_secret = Some(params.ephemeral_secret);
        self.local_conn_id = params.local_conn_id;
        self.initiator_eph = Some(parts.ephemeral);
        self.initiator_conn_id = parts.header.src_conn_id();
        self.attempts = 1;
        self.state = ResponderState::AwaitConfirmed;
        let deadline_at_ms = self.deadline_at_ms;
        Ok((
            self,
            vec![
                HandshakeAction::WriteDatagram(DatagramBytes::new(created_datagram)?),
                HandshakeAction::ArmDeadline {
                    kind: DeadlineKind::SessionCreated,
                    at_ms: now_ms.saturating_add(constants::SESSION_CREATED_RESEND_DELAYS_MS[0]),
                },
                HandshakeAction::ArmDeadline {
                    kind: DeadlineKind::Handshake,
                    at_ms: deadline_at_ms,
                },
            ],
        ))
    }

    fn answer_request_while_confirming(self, datagram: &mut [u8]) -> (Self, Vec<HandshakeAction>) {
        let parts = match parse_session_request(datagram, &self.config.intro_key) {
            Ok(parts) => parts,
            Err(_) => {
                return (
                    self,
                    vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                );
            }
        };
        if self.initiator_eph == Some(parts.ephemeral)
            && self.initiator_conn_id == parts.header.src_conn_id()
            && !self.created_datagram.is_empty()
        {
            let bytes = self.created_datagram.clone();
            return (
                self,
                vec![HandshakeAction::WriteDatagram(DatagramBytes(bytes))],
            );
        }
        (
            self,
            vec![HandshakeAction::DropSilently(DropCategory::Unexpected)],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_retry_for_request(
        self,
        header: &LongHeader,
        source: SocketAddr,
        request_length: usize,
        params: ResponderParams,
        retry_token_bytes: [u8; 8],
        store: &mut TokenStore,
        now_secs: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        let token = store
            .issue(source, now_secs, retry_token_bytes)
            .map_err(StateMachineError::from)?;
        let retry = build_retry(
            &self.config.intro_key,
            request_length,
            header.src_conn_id(),
            params.local_conn_id,
            params.packet_number,
            token.value(),
            params.timestamp,
            self.config.local_address,
            None,
            params.padding,
        )?;
        Ok((
            self,
            vec![HandshakeAction::WriteDatagram(DatagramBytes::new(retry)?)],
        ))
    }

    /// Handles one inbound SessionConfirmed fragment: accumulates into
    /// the bounded reassembly and, once complete, opens the static
    /// frame and payload, validates RouterInfo establishment, and
    /// reports the authenticated session.
    pub fn on_session_confirmed(
        mut self,
        mut datagram: Vec<u8>,
        source: SocketAddr,
        now_ms: u64,
        now_secs: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        if self.state != ResponderState::AwaitConfirmed {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::Unexpected)],
            ));
        }
        if now_ms >= self.deadline_at_ms {
            return Ok((
                self,
                vec![HandshakeAction::Terminate(
                    TerminateReason::HandshakeTimeout,
                )],
            ));
        }
        let confirmed_key = session_confirmed_header_key(
            &self
                .confirmed_chain_key
                .ok_or(StateMachineError::InvalidState)?,
        )?;
        if crate::crypto::remove_header_protection(
            &mut datagram,
            constants::SHORT_HEADER_LENGTH,
            self.config.intro_key.as_bytes(),
            &confirmed_key,
            false,
        )
        .is_err()
        {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
            ));
        }
        if datagram.len() < constants::SHORT_HEADER_LENGTH + constants::MIN_POST_HEADER_BYTES {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
            ));
        }
        let header =
            match SessionConfirmedHeader::decode(&datagram[..constants::SHORT_HEADER_LENGTH]) {
                Ok(header) => header,
                Err(_) => {
                    return Ok((
                        self,
                        vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                    ));
                }
            };
        if header.dst_conn_id() != self.local_conn_id {
            return Ok((
                self,
                vec![HandshakeAction::DropSilently(
                    DropCategory::ConnectionIdMismatch,
                )],
            ));
        }
        let fragment = datagram[constants::SHORT_HEADER_LENGTH..].to_vec();
        if self.reassembly.is_none() {
            self.reassembly = Some(match ConfirmedReassembly::new(header) {
                Ok(reassembly) => reassembly,
                Err(_) => {
                    return Ok((
                        self,
                        vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                    ));
                }
            });
        }
        let complete = {
            let reassembly = self
                .reassembly
                .as_mut()
                .ok_or(StateMachineError::InvalidState)?;
            match reassembly.add_fragment(header, fragment) {
                Ok(()) => reassembly.is_complete(),
                Err(HandshakeError::DuplicateFragment) => reassembly.is_complete(),
                Err(_) => {
                    return Ok((
                        self,
                        vec![HandshakeAction::DropSilently(DropCategory::Malformed)],
                    ));
                }
            }
        };
        if !complete {
            return Ok((self, vec![]));
        }
        let reassembly = self
            .reassembly
            .take()
            .ok_or(StateMachineError::InvalidState)?;
        let jumbo = match reassembly.reassemble() {
            Ok(jumbo) => jumbo,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::Terminate(
                        TerminateReason::RouterInfoRejected,
                    )],
                ));
            }
        };
        let (static_frame, payload_ct) = match split_confirmed_jumbo(&jumbo) {
            Ok(split) => split,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::Terminate(
                        TerminateReason::RouterInfoRejected,
                    )],
                ));
            }
        };
        let transcript = self
            .transcript
            .take()
            .ok_or(StateMachineError::InvalidState)?;
        let (transcript, alice_static) = match transcript.accept_confirmed_static(static_frame) {
            Ok(value) => value,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::Terminate(
                        TerminateReason::AuthenticationFailed,
                    )],
                ));
            }
        };
        let ephemeral_secret = self
            .ephemeral_secret
            .take()
            .ok_or(StateMachineError::InvalidState)?;
        let se = ephemeral_secret
            .diffie_hellman(alice_static.as_bytes())
            .map_err(|_| {
                StateMachineError::Handshake(HandshakeError::Crypto(
                    Ssu2CryptoError::AuthenticationFailed,
                ))
            })?;
        let (transcript, payload) = match transcript.open_confirmed_payload(se, payload_ct) {
            Ok(value) => value,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::Terminate(
                        TerminateReason::AuthenticationFailed,
                    )],
                ));
            }
        };
        let router_info_bytes = match require_first_router_info(&payload) {
            Ok(bytes) => bytes,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::Terminate(
                        TerminateReason::RouterInfoRejected,
                    )],
                ));
            }
        };
        let peer = match validate_router_info(
            &router_info_bytes,
            constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES,
            self.config.expected_peer_hash,
            &alice_static,
            RouterInfoFreshness::default_for(now_secs),
        ) {
            Ok(peer) => peer,
            Err(_) => {
                return Ok((
                    self,
                    vec![HandshakeAction::Terminate(
                        TerminateReason::RouterInfoRejected,
                    )],
                ));
            }
        };
        let keys = transcript.split()?;
        let session = AuthenticatedSsu2Session {
            peer,
            keys,
            local_conn_id: self.local_conn_id,
            remote_conn_id: self.initiator_conn_id,
            peer_endpoint: source,
            local_mtu: self.config.local_mtu,
        };
        Ok((self, vec![HandshakeAction::Established(session)]))
    }

    fn local_static_public(&self) -> Result<Ssu2PublicKey, StateMachineError> {
        Ssu2PublicKey::new(self.config.static_secret.public_bytes())
            .map_err(StateMachineError::Crypto)
    }

    /// Handles a retransmit/timeout firing at `now_ms`: resends the
    /// identical SessionCreated while attempts remain, else terminates.
    pub fn on_timeout(
        mut self,
        now_ms: u64,
    ) -> Result<(Self, Vec<HandshakeAction>), StateMachineError> {
        if now_ms >= self.deadline_at_ms {
            return Ok((
                self,
                vec![HandshakeAction::Terminate(
                    TerminateReason::HandshakeTimeout,
                )],
            ));
        }
        if self.state != ResponderState::AwaitConfirmed || self.created_datagram.is_empty() {
            return Ok((self, vec![]));
        }
        let next_index = usize::from(self.attempts);
        if next_index >= constants::SESSION_CREATED_RESEND_DELAYS_MS.len()
            || self.attempts >= constants::MAX_HANDSHAKE_ATTEMPTS
        {
            return Ok((
                self,
                vec![HandshakeAction::Terminate(
                    TerminateReason::RetriesExhausted,
                )],
            ));
        }
        self.attempts += 1;
        let at_ms = now_ms.saturating_add(constants::SESSION_CREATED_RESEND_DELAYS_MS[next_index]);
        let resend = DatagramBytes::new(self.created_datagram.clone())?;
        Ok((
            self,
            vec![
                HandshakeAction::WriteDatagram(resend),
                HandshakeAction::ArmDeadline {
                    kind: DeadlineKind::SessionCreated,
                    at_ms,
                },
            ],
        ))
    }

    /// Cancels establishment; all retained state is released with the machine.
    pub fn cancel(self) -> HandshakeAction {
        HandshakeAction::Terminate(TerminateReason::Cancelled)
    }
}

/// Deprotects a working copy of the first 16 header bytes with the two
/// intro-derived keys and reads the message type byte. Used only to
/// route TokenRequest/SessionRequest/Retry handling; full parsing
/// re-validates everything afterwards.
fn deprotect_type_byte(
    datagram: &[u8],
    k_header_1: &[u8; constants::KEY_LENGTH],
    k_header_2: &[u8; constants::KEY_LENGTH],
) -> Result<MessageType, ()> {
    use crate::crypto::chacha_mask;
    if datagram.len() < constants::MIN_DATAGRAM_LENGTH {
        return Err(());
    }
    let length = datagram.len();
    let iv1: [u8; 12] = datagram[length - 24..length - 12]
        .try_into()
        .map_err(|_| ())?;
    let mask = chacha_mask(k_header_1, &iv1);
    let mut first: [u8; 16] = datagram[..16].try_into().map_err(|_| ())?;
    for (byte, mask) in first[..8].iter_mut().zip(mask.iter()) {
        *byte ^= *mask;
    }
    let iv2: [u8; 12] = datagram[length - 12..length].try_into().map_err(|_| ())?;
    let mask = chacha_mask(k_header_2, &iv2);
    for (byte, mask) in first[8..16].iter_mut().zip(mask.iter()) {
        *byte ^= *mask;
    }
    MessageType::from_u8(first[12]).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_stay_debuggable_without_secrets() {
        let action = HandshakeAction::DropSilently(DropCategory::BadToken);
        assert_eq!(format!("{action:?}"), "DropSilently(BadToken)");
    }
}
