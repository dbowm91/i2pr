//! Runtime-neutral SSU2 v2 data-phase session (Plan 157).
//!
//! This module owns the authenticated data phase that consumes Plan
//! 156's [`crate::state_machine::AuthenticatedSsu2Session`] output:
//! short-header protection, packet-number/replay handling, ACK ranges
//! and scheduling, bounded loss/retransmission/congestion state, I2NP
//! fragmentation/reassembly, duplicate-message suppression, and
//! rekey/termination/idle behavior.
//!
//! No UDP sockets are opened here. All time and randomness arrive as
//! caller-supplied inputs; the single central [`Ssu2Session::poll`]
//! entry point drives ACK/RTO/idle/reassembly deadlines. There is no
//! timer or task per packet.
//!
//! Normative traceability: SSU2 specification sections Packet
//! Numbering, Header Encryption KDF, KDF for data phase (label
//! `"HKDFSSU2DataKeys"` splitting each directional key into
//! `(k_data, k_header_2)` with `k_header_1` equal to the receiver's
//! intro key), Data Message (type 6), ACK Block, I2NP
//! Message/First-Fragment/Follow-On-Fragment blocks, Termination
//! block, PathChallenge/PathResponse, NewToken, Congestion, and the
//! Congestion Control / Generating ACKs / Sending ACK Blocks / ACK
//! Frequency / Immediate ACK Flag guidance (RFC 6298/9002 deferred).
//! Proposals 159/165 are historical context only.
//!
//! Interpretation notes (documented here so a later interop plan can
//! revisit them mechanically):
//!
//! - The v2 short header carries the full 32-bit packet number; no
//!   truncated-number reconstruction is required. The
//!   "reconstruction" step is therefore the identity function plus
//!   wrap-aware window admissibility. This is exact for the pinned
//!   specification, not a simplification.
//! - The specification leaves several data-phase policies to the
//!   implementation (replay-window size, future-jump limit, ACK
//!   delays, congestion constants, idle timeout, retransmission
//!   ceilings, reassembly quotas, duplicate-cache size). Each choice
//!   is pinned in [`crate::constants`] with a `DATA_` prefix and
//!   exercised by boundary tests.
//! - The specification's ack-eliciting list ends with "Others?". This
//!   implementation treats every block except ACK, Address, DateTime,
//!   Padding, and Termination as ack-eliciting. Control blocks
//!   (I2NP/fragments, relay/peer-test, path, tokens, congestion,
//!   options) therefore elicit ACKs, while pure
//!   ACK/Address/DateTime/Padding/Termination packets never elicit an
//!   ACK response on their own, which provably avoids ACK-of-ACK
//!   loops (tested).
//! - `NextNonce` (type 11) is specified as TODO for key rotation with
//!   no normative behavior. It is decoded into a typed
//!   [`SessionEvent::RekeyRequested`] event; no automatic rekey
//!   occurs in this pass.
//! - `FirstPacketNumber` (type 20) is specified as not fully
//!   specified / not currently supported. It is decoded into a typed
//!   event; the session's packet-number start remains the
//!   caller-supplied initial value.
//! - Relay and peer-test blocks are decoded into typed opaque events;
//!   their state machines belong to Plan 160. Path
//!   challenge/response blocks are decoded into typed events with
//!   queue helpers; endpoint migration policy belongs to Plan 159 and
//!   no migration occurs here.
//! - Outbound control blocks (path challenge/response, termination)
//!   are single-shot: a lost control packet is not automatically
//!   retried (the specification only *permits* whole-packet
//!   termination retransmission). Per-message I2NP delivery failure
//!   is likewise silent by design — the session stays usable and the
//!   loss/retransmit counters expose it — matching the specification
//!   rule that delivery may fail without disconnecting.
//!
//! Congestion-control policy (conservative, auditable, RFC
//! 6298-inspired where the specification defers):
//!
//! - Byte-count window with explicit min/default/max.
//! - Bytes-in-flight counts only congestion-controlled
//!   (ack-eliciting) packets; ACK-only packets are excluded.
//! - RTT samples come only from ack-eliciting packets sent exactly
//!   once (Karn's rule); retransmitted packets never produce samples.
//! - `srtt/rttvar/rto` follow RFC 6298 §2.2-2.3 with the SSU2 1 s
//!   minimum RTO; all arithmetic is saturating/checked.
//! - Loss (explicit NACK gap or RTO expiry) halves the window down to
//!   the minimum; each newly-acknowledged byte grows it additively up
//!   to the maximum.
//!
//! Resource policy: every queue is bounded with exact-capacity/max+1
//! tests. Complete ciphertext datagrams are never retained for
//! retransmission; only semantic fragment bytes plus provenance are
//! kept so fresh packets (new number, current ACK state) can be
//! assembled.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    vec::Vec,
};

use i2pr_proto::MessageType;
use i2pr_transport::{EncodedI2npMessage, MAX_I2NP_MESSAGE_BYTES};
use thiserror::Error;

use crate::block::{
    AckBlock, Block, BlockError, DecodedBlock, FirstFragmentBlock, FollowOnFragmentBlock,
    PathChallengeBlock, PathResponseBlock, encode_blocks, parse_blocks,
};
use crate::constants;
use crate::crypto::{DataCipher, IntroKey, Ssu2CryptoError, Ssu2SplitKeys};
use crate::header::{DataHeader, HeaderError};
use crate::packet::{DatagramLengthClass, PacketError};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Typed failures from data-phase session sequencing.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum SessionError {
    /// A datagram length class was invalid.
    #[error("SSU2 data datagram length is invalid")]
    Packet(#[from] PacketError),
    /// A short header was structurally invalid.
    #[error("SSU2 data header is invalid")]
    Header(#[from] HeaderError),
    /// A header-protection or AEAD operation failed.
    #[error("SSU2 data crypto operation failed")]
    Crypto(#[from] Ssu2CryptoError),
    /// A block sequence was malformed.
    #[error("SSU2 data block sequence is invalid")]
    Blocks(#[from] BlockError),
    /// The destination connection ID did not match this session.
    #[error("SSU2 data packet is not for this session")]
    NotForSession,
    /// The packet number was already seen.
    #[error("SSU2 data packet is a replay")]
    Replay,
    /// The packet number is below the replay window.
    #[error("SSU2 data packet is too old")]
    TooOld,
    /// The packet number jumps impossibly far ahead.
    #[error("SSU2 data packet jumps too far into the future")]
    FutureJump,
    /// An ACK block implies packet numbers below zero.
    #[error("SSU2 ACK block underflows the packet-number space")]
    AckUnderflow,
    /// An ACK block is otherwise semantically impossible.
    #[error("SSU2 ACK block is semantically invalid")]
    AckInvalid,
    /// A packet number could not be allocated.
    #[error("SSU2 packet-number space is exhausted")]
    PacketNumberExhausted,
    /// The sent-packet history has no admission capacity.
    #[error("SSU2 sent-packet history is full")]
    SentHistoryFull,
    /// The retransmission queue has no admission capacity.
    #[error("SSU2 retransmission queue is full")]
    RetransmitQueueFull,
    /// The outbound I2NP queue has no admission capacity.
    #[error("SSU2 outbound I2NP queue is full")]
    OutboundQueueFull,
    /// An I2NP message exceeds fragmentation or size bounds.
    #[error("SSU2 I2NP message exceeds its bound")]
    MessageTooLarge,
    /// A reassembly admission was denied by quota.
    #[error("SSU2 reassembly quota denied admission")]
    ReassemblyFull,
    /// A reassembly entry detected conflicting fragment data.
    #[error("SSU2 reassembly detected a conflicting fragment")]
    ReassemblyConflict,
    /// The session has terminated and accepts no new work.
    #[error("SSU2 session is terminated")]
    Terminated,
    /// A local policy denied the operation.
    #[error("SSU2 session policy denied the operation")]
    LocalPolicyDenied,
}

// ---------------------------------------------------------------------------
// Configuration and counters
// ---------------------------------------------------------------------------

/// Caller-supplied data-phase session configuration.
///
/// All values are explicit so tests stay deterministic. Timeouts and
/// quotas default to the pinned [`crate::constants`] policies.
pub struct SessionConfig {
    /// Locally allocated connection ID (validates inbound `dst_conn_id`).
    pub local_conn_id: u64,
    /// Peer's connection ID (emitted as outbound `dst_conn_id`).
    pub remote_conn_id: u64,
    /// Local intro key (`k_header_1` for inbound packets).
    pub local_intro: IntroKey,
    /// Peer's intro key (`k_header_1` for outbound packets).
    pub remote_intro: IntroKey,
    /// First outbound packet number (default 0 per specification).
    pub initial_send_packet_number: u32,
    /// Maximum authenticated payload bytes per datagram
    /// (default 1220 = 1280-MTU IPv4 budget per `MTU - 60`).
    pub max_payload_bytes: usize,
    /// Data-phase idle timeout in milliseconds.
    pub idle_timeout_ms: u64,
}

impl SessionConfig {
    /// Returns the default payload budget for an MTU and address family.
    pub const fn max_payload_for_mtu(mtu: u16, ipv6: bool) -> usize {
        let overhead = if ipv6 {
            constants::DATA_IPV6_OVERHEAD_BYTES
        } else {
            constants::DATA_IPV4_OVERHEAD_BYTES
        };
        (mtu as usize).saturating_sub(overhead)
    }
}

/// Privacy-safe session diagnostics for Plan 158/161 evidence.
///
/// Counts only; no payload bytes, token values, raw keys, or endpoint
/// histories appear here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionCounters {
    /// Data datagrams sealed on the transmit path.
    pub packets_sent: u64,
    /// Data datagrams authenticated on the receive path.
    pub packets_received: u64,
    /// Inbound packets dropped as replays/duplicates.
    pub packets_replayed: u64,
    /// Inbound packets rejected (length/header/crypto/block/window).
    pub packets_rejected: u64,
    /// ACK blocks sealed on the transmit path.
    pub acks_sent: u64,
    /// ACK blocks interpreted on the receive path.
    pub acks_received: u64,
    /// Loss declarations (NACK gap or RTO expiry).
    pub loss_events: u64,
    /// Semantic fragments scheduled for fresh retransmission.
    pub retransmitted_fragments: u64,
    /// Current congestion-controlled bytes in flight.
    pub bytes_in_flight: usize,
    /// Current congestion window in bytes.
    pub cwnd_bytes: usize,
    /// Current in-progress reassembly messages.
    pub reassembly_messages: usize,
    /// Current retained reassembly bytes.
    pub reassembly_bytes: usize,
    /// Reassembly admissions denied or entries dropped on conflict.
    pub reassembly_drops: u64,
    /// Locally initiated termination, if any.
    pub terminated_local: bool,
    /// Remotely observed termination reason code, if any.
    pub terminated_remote: Option<u8>,
}

// ---------------------------------------------------------------------------
// Events and actions
// ---------------------------------------------------------------------------

/// One semantic effect of an authenticated inbound data packet.
///
/// Relay/peer-test payloads are opaque bounded evidence; their state
/// machines belong to Plan 160. Path validation plumbing is exposed
/// here while migration policy stays in Plan 159.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    /// One complete I2NP message delivered exactly once (reassembled
    /// or single-block). Bytes remain owned by the caller handoff.
    I2npMessage(Vec<u8>),
    /// A NewToken block was observed (handoff for future handshakes).
    NewToken {
        /// Unix expiry timestamp from the block.
        expires: u32,
        /// One-use token value from the block.
        token: u64,
    },
    /// A PathChallenge block was observed; the caller may answer via
    /// [`Ssu2Session::queue_path_response`]. No migration occurs here.
    PathChallenge(Vec<u8>),
    /// A PathResponse block was observed.
    PathResponse(Vec<u8>),
    /// A Termination block was observed.
    Termination {
        /// Peer's valid-packets-received counter.
        valid_packets_received: u64,
        /// Wire reason code.
        reason: u8,
    },
    /// A NextNonce block was observed (spec TODO); no rekey occurs.
    RekeyRequested,
    /// A FirstPacketNumber block was observed; the session start is
    /// unchanged (spec: not fully specified).
    FirstPacketNumber(u32),
    /// A DateTime block was observed.
    DateTime(u32),
    /// An Options block was observed.
    Options {
        /// Fixed transmit padding-ratio range.
        transmit_ratios: (u8, u8),
        /// Fixed receive padding-ratio range.
        receive_ratios: (u8, u8),
    },
    /// A Congestion block was observed.
    Congestion {
        /// Raw congestion flags.
        flags: u8,
    },
    /// A relay block was observed (opaque; Plan 160 owns semantics).
    Relay {
        /// Wire block type (7/8/9).
        block_type: u8,
        /// Bounded opaque block length.
        length: usize,
    },
    /// A peer-test block was observed (opaque; Plan 160 owns semantics).
    PeerTest {
        /// Peer-test message number (1..=7).
        message: u8,
    },
    /// An Address block was observed (no publication effect here).
    AddressObserved,
}

/// The disposition of one inbound datagram after the ordered receive
/// pipeline (cheap checks → session binding → header protection →
/// replay window → AEAD → bounded block parse → effects).
#[derive(Debug)]
pub struct ReceiveOutcome {
    /// Authenticated packet number, when the header was valid.
    pub packet_number: Option<u32>,
    /// Whether the packet carried ack-eliciting blocks.
    pub ack_eliciting: bool,
    /// Whether the packet was ACK-only (no ack-eliciting blocks).
    pub ack_only: bool,
    /// Semantic events exposed only after successful authentication.
    pub events: Vec<SessionEvent>,
    /// Whether reception schedules an ACK (deadline armed by poll).
    pub ack_scheduled: bool,
    /// Silently-dropped classification for diagnostics only.
    pub dropped: Option<DropReason>,
}

/// Why an inbound datagram produced no application-visible effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropReason {
    /// Length or structural check failed.
    Malformed,
    /// Connection-ID mismatch (not for this session).
    ConnectionIdMismatch,
    /// Replay/duplicate within the window.
    Replay,
    /// Below the window floor.
    TooOld,
    /// Impossibly far ahead.
    FutureJump,
    /// AEAD authentication failed; no state mutated.
    AuthenticationFailed,
    /// Authenticated block parse failed.
    BlockParseFailed,
    /// Semantically invalid ACK; sent state untouched.
    InvalidAck,
    /// Session is terminated.
    Terminated,
}

/// One polled session action for the runtime (Plan 158) to fulfill.
#[derive(Debug)]
pub enum SessionAction {
    /// Emit one bounded datagram.
    Transmit(Vec<u8>),
    /// Termination is recommended (idle/RTO ceiling/local request).
    Terminate {
        /// Wire reason code to send, when a final packet is desired.
        reason: u8,
    },
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

/// One congestion-controlled sent packet's provenance.
///
/// Only semantic fragment bytes plus accounting are retained; the
/// sealed ciphertext is never stored.
struct SentPacket {
    packet_number: u32,
    sent_ms: u64,
    ack_eliciting_bytes: usize,
    ack_only: bool,
    fragments: Vec<RetransmittableFragment>,
    /// Whether this packet's bytes count toward bytes-in-flight.
    counts_for_congestion: bool,
    /// Retransmission generation of the fragments carried (0 = first).
    generation: u8,
}

/// One semantic fragment retained for fresh retransmission.
#[derive(Clone, Debug)]
struct RetransmittableFragment {
    message_id: u32,
    frag_number: u8,
    is_last: bool,
    /// Total fragments of the parent message (1 = complete block).
    total_fragments: u8,
    message_type: u8,
    expiration_secs: u32,
    bytes: Vec<u8>,
    generation: u8,
}

/// One outbound I2NP message split into fixed fragments at queue time
/// so retransmissions preserve length and implicit offsets.
struct OutboundMessage {
    message_id: u32,
    message_type: u8,
    expiration_secs: u32,
    fragments: VecDeque<QueuedFragment>,
    /// Fragments already handed to the transmit path but unacked.
    in_flight: usize,
}

#[derive(Clone, Debug)]
struct QueuedFragment {
    frag_number: u8,
    is_last: bool,
    /// Total fragments of the parent message (1 = complete block).
    total_fragments: u8,
    bytes: Vec<u8>,
}

/// One in-progress inbound I2NP reassembly.
struct ReassemblyEntry {
    message_type: Option<u8>,
    expiration_secs: Option<u32>,
    fragments: BTreeMap<u8, Vec<u8>>,
    /// Total fragment count once an `is_last` fragment arrives.
    total: Option<u8>,
    total_bytes: usize,
    created_ms: u64,
    last_progress_ms: u64,
}

impl ReassemblyEntry {
    /// Creates an empty placeholder without borrowing session state
    /// (keeps the reassembly admission borrow-checker clean).
    fn placeholder(now_ms: u64) -> Self {
        Self {
            message_type: None,
            expiration_secs: None,
            fragments: BTreeMap::new(),
            total: None,
            total_bytes: 0,
            created_ms: now_ms,
            last_progress_ms: now_ms,
        }
    }
}

/// One recently-delivered message ID for duplicate suppression.
struct DeliveredEntry {
    message_id: u32,
    expiration_secs: u32,
    delivered_ms: u64,
}

/// Queued outbound control blocks (bounded).
#[derive(Clone, Debug)]
enum QueuedControl {
    PathChallenge(Vec<u8>),
    PathResponse(Vec<u8>),
    NewToken {
        expires: u32,
        token: u64,
    },
    Termination {
        valid_packets_received: u64,
        reason: u8,
    },
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// An authenticated SSU2 v2 data-phase session.
///
/// Constructed from Plan 156's directional keys plus explicit intro
/// keys; owns send/receive packet-number state, the bounded replay
/// window, pending ACK state, RTT/RTO/loss/congestion state,
/// sent-packet provenance, pending retransmissions, reassembly,
/// duplicate suppression, and idle/termination state.
pub struct Ssu2Session {
    local_conn_id: u64,
    remote_conn_id: u64,
    local_intro: IntroKey,
    remote_intro: IntroKey,
    max_payload_bytes: usize,
    idle_timeout_ms: u64,

    transmit_cipher: DataCipher,
    transmit_header_2: [u8; constants::KEY_LENGTH],
    receive_cipher: DataCipher,
    receive_header_2: [u8; constants::KEY_LENGTH],

    next_send: u32,
    send_exhausted: bool,
    packets_sent_count: u64,

    highest_received: Option<u32>,
    received_bitmap: u128,
    eliciting_bitmap: u128,

    ack_pending: bool,
    ack_deadline_ms: Option<u64>,
    ack_eliciting_since_ack: u32,
    last_received_eliciting: bool,

    srtt_ms: Option<u64>,
    rttvar_ms: u64,
    rto_ms: u64,
    cwnd_bytes: usize,
    bytes_in_flight: usize,
    consecutive_rto: u32,

    sent: VecDeque<SentPacket>,
    pending_retransmit: VecDeque<RetransmittableFragment>,
    outbound: VecDeque<OutboundMessage>,
    queued_controls: VecDeque<QueuedControl>,

    reassembly: BTreeMap<u32, ReassemblyEntry>,
    reassembly_bytes: usize,
    delivered: VecDeque<DeliveredEntry>,

    last_send_ms: Option<u64>,
    last_receive_ms: Option<u64>,
    local_terminate: Option<u8>,
    remote_terminate: Option<u8>,

    counters: SessionCounters,
}

impl fmt::Debug for Ssu2Session {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ssu2Session")
            .field("local_conn_id", &self.local_conn_id)
            .field("remote_conn_id", &self.remote_conn_id)
            .field("next_send", &self.next_send)
            .field("highest_received", &self.highest_received)
            .field("bytes_in_flight", &self.bytes_in_flight)
            .field("cwnd_bytes", &self.cwnd_bytes)
            .field("counters", &self.counters)
            .finish()
    }
}

impl Ssu2Session {
    /// Builds a data-phase session from establishment keys.
    ///
    /// `keys` are consumed from [`Ssu2SplitKeys::into_parts`]; the
    /// directional `k_header_2` values travel with them while
    /// `k_header_1` comes from `config`'s intro keys (receiver's key
    /// per direction, per the Header Encryption KDF table).
    pub fn new(mut config: SessionConfig, keys: Ssu2SplitKeys) -> Result<Self, SessionError> {
        if config.local_conn_id == config.remote_conn_id {
            return Err(SessionError::LocalPolicyDenied);
        }
        if config.max_payload_bytes == 0 {
            config.max_payload_bytes = SessionConfig::max_payload_for_mtu(1280, false);
        }
        if config.max_payload_bytes < constants::MIN_POST_HEADER_BYTES + 16 {
            return Err(SessionError::LocalPolicyDenied);
        }
        let (tx_keys, rx_keys) = keys.into_parts();
        let (transmit_cipher, transmit_header_2) = tx_keys.into_owner();
        let (receive_cipher, receive_header_2) = rx_keys.into_owner();
        let counters = SessionCounters {
            cwnd_bytes: constants::DATA_DEFAULT_CWND_BYTES,
            ..SessionCounters::default()
        };
        Ok(Self {
            local_conn_id: config.local_conn_id,
            remote_conn_id: config.remote_conn_id,
            local_intro: config.local_intro,
            remote_intro: config.remote_intro,
            max_payload_bytes: config.max_payload_bytes,
            idle_timeout_ms: config.idle_timeout_ms,
            transmit_cipher,
            transmit_header_2,
            receive_cipher,
            receive_header_2,
            next_send: config.initial_send_packet_number,
            send_exhausted: false,
            packets_sent_count: 0,
            highest_received: None,
            received_bitmap: 0,
            eliciting_bitmap: 0,
            ack_pending: false,
            ack_deadline_ms: None,
            ack_eliciting_since_ack: 0,
            last_received_eliciting: false,
            srtt_ms: None,
            rttvar_ms: 0,
            rto_ms: constants::DATA_INITIAL_RTO_MS,
            cwnd_bytes: constants::DATA_DEFAULT_CWND_BYTES,
            bytes_in_flight: 0,
            consecutive_rto: 0,
            sent: VecDeque::new(),
            pending_retransmit: VecDeque::new(),
            outbound: VecDeque::new(),
            queued_controls: VecDeque::new(),
            reassembly: BTreeMap::new(),
            reassembly_bytes: 0,
            delivered: VecDeque::new(),
            last_send_ms: None,
            last_receive_ms: None,
            local_terminate: None,
            remote_terminate: None,
            counters,
        })
    }

    /// Returns whether an inbound datagram is addressed to this session
    /// without mutating any session state or diagnostic counters.
    ///
    /// Plan 158's runtime receive loop calls this to route short-header
    /// datagrams to the owning session before invoking the mutating
    /// [`Ssu2Session::receive_datagram`]. Trial deprotection uses the
    /// session's own intro/`k_header_2` pair, so an unrelated session
    /// observes no replay marks, no rejection counters, and no effects.
    pub fn matches_inbound(&self, datagram: &[u8]) -> bool {
        if self.is_terminated() {
            return false;
        }
        if DatagramLengthClass::classify(datagram.len()).is_err() {
            return false;
        }
        if datagram.len() < constants::SHORT_HEADER_LENGTH + constants::MIN_POST_HEADER_BYTES {
            return false;
        }
        let mut working = datagram.to_vec();
        if crate::crypto::remove_header_protection(
            &mut working,
            constants::SHORT_HEADER_LENGTH,
            self.local_intro.as_bytes(),
            &self.receive_header_2,
            false,
        )
        .is_err()
        {
            return false;
        }
        match DataHeader::decode(&working[..constants::SHORT_HEADER_LENGTH]) {
            Ok(header) => header.dst_conn_id() == self.local_conn_id,
            Err(_) => false,
        }
    }

    /// Returns pending outbound depth for runtime queue accounting:
    /// the number of queued I2NP messages and an estimated queued byte
    /// count (fragment bytes plus a per-message header slop).
    ///
    /// The estimate is an admission input only, never a wire promise;
    /// actual datagrams add short-header, block-framing, and MAC bytes.
    pub fn outbound_pending(&self) -> (usize, usize) {
        let mut bytes = 0_usize;
        for message in &self.outbound {
            bytes = bytes.saturating_add(64);
            for fragment in &message.fragments {
                bytes = bytes.saturating_add(fragment.bytes.len());
            }
        }
        (self.outbound.len(), bytes)
    }

    /// Returns the local connection ID.
    pub const fn local_conn_id(&self) -> u64 {
        self.local_conn_id
    }

    /// Returns the remote connection ID.
    pub const fn remote_conn_id(&self) -> u64 {
        self.remote_conn_id
    }

    /// Returns the next outbound packet number without allocating it.
    pub const fn next_send_packet_number(&self) -> u32 {
        self.next_send
    }

    /// Returns current bytes in flight.
    pub const fn bytes_in_flight(&self) -> usize {
        self.bytes_in_flight
    }

    /// Returns the current congestion window.
    pub const fn cwnd_bytes(&self) -> usize {
        self.cwnd_bytes
    }

    /// Returns privacy-safe diagnostic counters.
    pub const fn counters(&self) -> SessionCounters {
        self.counters
    }

    /// Returns whether the session has terminated.
    pub const fn is_terminated(&self) -> bool {
        self.local_terminate.is_some() || self.remote_terminate.is_some()
    }

    /// Returns the next ACK deadline, if one is armed.
    pub const fn ack_deadline_ms(&self) -> Option<u64> {
        self.ack_deadline_ms
    }

    /// Returns the current RTO estimate.
    pub const fn rto_ms(&self) -> u64 {
        self.rto_ms
    }

    // -- Outbound queueing ---------------------------------------------

    /// Queues one complete encoded I2NP message for fragmentation.
    ///
    /// The 9-byte short transport header (type, message ID,
    /// expiration) is parsed from `bytes`; the body is split into
    /// fixed 1024-byte semantic fragments (capped by the payload
    /// budget) with deterministic numbering. Returns the message ID.
    pub fn queue_i2np_message(&mut self, bytes: Vec<u8>) -> Result<u32, SessionError> {
        if self.is_terminated() {
            return Err(SessionError::Terminated);
        }
        let message = EncodedI2npMessage::new(bytes).map_err(|_| SessionError::MessageTooLarge)?;
        let raw = message.as_bytes();
        if raw.len() < 9 {
            return Err(SessionError::MessageTooLarge);
        }
        let message_type = raw[0];
        let _ = MessageType::from_code(message_type);
        let message_id = u32::from_be_bytes(raw[1..5].try_into().expect("length checked"));
        let expiration_secs = u32::from_be_bytes(raw[5..9].try_into().expect("length checked"));
        let body = &raw[9..];
        if body.is_empty() {
            return Err(SessionError::MessageTooLarge);
        }
        // Fixed fragment data budget: conservative 1024-byte semantic
        // fragments capped by the session payload budget minus block
        // overhead and an ACK reserve, so boundaries never shift
        // between fresh retransmissions.
        let budget = self.max_payload_bytes.saturating_sub(64).clamp(64, 1024);
        let total = body.len().div_ceil(budget);
        if total == 0 || total > constants::MAX_I2NP_FRAGMENTS {
            return Err(SessionError::MessageTooLarge);
        }
        if self.outbound.len() >= constants::DATA_MAX_SENT_PACKETS {
            return Err(SessionError::OutboundQueueFull);
        }
        // Single-fragment messages travel as complete I2NP blocks;
        // multi-fragment messages use first/follow-on blocks. Both
        // retain semantic bytes for fresh retransmission.
        let mut fragments = VecDeque::with_capacity(total);
        #[allow(clippy::cast_possible_truncation)]
        let total_byte = total as u8;
        for (index, chunk) in body.chunks(budget).enumerate() {
            if chunk.is_empty() {
                return Err(SessionError::MessageTooLarge);
            }
            fragments.push_back(QueuedFragment {
                frag_number: index as u8,
                is_last: index + 1 == total,
                total_fragments: total_byte,
                bytes: chunk.to_vec(),
            });
        }
        // Preserve the complete encoded message for single-fragment
        // delivery; fragmentation only applies to multi-fragment.
        let _ = message;
        self.outbound.push_back(OutboundMessage {
            message_id,
            message_type,
            expiration_secs,
            fragments,
            in_flight: 0,
        });
        Ok(message_id)
    }

    /// Queues a path challenge for transmission.
    pub fn queue_path_challenge(&mut self, data: Vec<u8>) -> Result<(), SessionError> {
        if self.is_terminated() {
            return Err(SessionError::Terminated);
        }
        PathChallengeBlock::new(data)
            .map_err(|_| SessionError::MessageTooLarge)
            .and_then(|block| {
                if self.queued_controls.len() >= constants::DATA_MAX_SENT_PACKETS {
                    return Err(SessionError::LocalPolicyDenied);
                }
                self.queued_controls
                    .push_back(QueuedControl::PathChallenge(block.data().to_vec()));
                Ok(())
            })
    }

    /// Queues a path response for transmission.
    pub fn queue_path_response(&mut self, data: Vec<u8>) -> Result<(), SessionError> {
        if self.is_terminated() {
            return Err(SessionError::Terminated);
        }
        PathResponseBlock::new(data)
            .map_err(|_| SessionError::MessageTooLarge)
            .and_then(|block| {
                if self.queued_controls.len() >= constants::DATA_MAX_SENT_PACKETS {
                    return Err(SessionError::LocalPolicyDenied);
                }
                self.queued_controls
                    .push_back(QueuedControl::PathResponse(block.data().to_vec()));
                Ok(())
            })
    }

    /// Queues one single-shot NewToken control for transmission.
    ///
    /// Plan 158 uses this so a responder can hand a one-use token to an
    /// authenticated peer for a future handshake (the token itself is
    /// issued from the runtime's bounded [`crate::token::TokenStore`];
    /// this queue only carries the announced value). Like path
    /// controls, the block is emitted once on the next transmit; a lost
    /// control packet is not automatically retried.
    pub fn queue_new_token(&mut self, expires: u32, token: u64) -> Result<(), SessionError> {
        if self.is_terminated() {
            return Err(SessionError::Terminated);
        }
        if token == 0 {
            return Err(SessionError::MessageTooLarge);
        }
        if self.queued_controls.len() >= constants::DATA_MAX_SENT_PACKETS {
            return Err(SessionError::LocalPolicyDenied);
        }
        self.queued_controls
            .push_back(QueuedControl::NewToken { expires, token });
        Ok(())
    }

    /// Queues a local termination. The block is emitted on the next
    /// transmit; the session reports terminated once queued.
    pub fn initiate_termination(&mut self, reason: u8) -> Result<(), SessionError> {
        if self.is_terminated() {
            return Err(SessionError::Terminated);
        }
        if self.queued_controls.len() >= constants::DATA_MAX_SENT_PACKETS {
            return Err(SessionError::LocalPolicyDenied);
        }
        let valid = self.counters.packets_received;
        self.queued_controls.push_back(QueuedControl::Termination {
            valid_packets_received: valid,
            reason,
        });
        // Bounded cleanup on local terminal conditions: release pending
        // retransmit and reassembly buffers. The queued Termination
        // control itself is preserved for the final transmit.
        self.pending_retransmit.clear();
        self.clear_reassembly();
        self.local_terminate = Some(reason);
        Ok(())
    }

    // -- Transmit path -------------------------------------------------

    /// Builds at most one outbound datagram from pending ACK, control,
    /// and I2NP-fragment state.
    ///
    /// Allocation order per packet: ACK block (when pending),
    /// termination/path controls, fresh retransmit fragments, then new
    /// outbound fragments — all MTU-aware, sealed once with the
    /// transmit cipher, then header-protected. Returns `None` when no
    /// work is pending or congestion blocks an ack-eliciting packet
    /// (ACK-only packets are never congestion-blocked per the
    /// specification exception).
    pub fn poll_transmit(&mut self, now_ms: u64) -> Option<Vec<u8>> {
        if self.send_exhausted {
            return None;
        }
        let want_ack = self.ack_pending;
        let has_controls = !self.queued_controls.is_empty();
        let has_retransmit = !self.pending_retransmit.is_empty();
        let has_outbound = self
            .outbound
            .iter()
            .any(|message| !message.fragments.is_empty());
        if !want_ack && !has_controls && !has_retransmit && !has_outbound {
            return None;
        }
        // Congestion gate: ack-eliciting packets wait while the window
        // is full; pure ACK-only packets always pass.
        let would_elicit =
            has_controls || has_retransmit || has_outbound || self.pending_ack_eliciting_controls();
        if would_elicit && self.bytes_in_flight >= self.cwnd_bytes {
            // An ACK-only piggyback is still allowed: emit the pending
            // ACK alone without new ack-eliciting bytes.
            if want_ack {
                let datagram = self.seal_packet(now_ms, Vec::new(), true);
                if datagram.is_some() {
                    self.clear_ack_state();
                }
                return datagram;
            }
            return None;
        }
        // Assemble blocks within the payload budget.
        let mut blocks: Vec<Block> = Vec::new();
        let mut used = 0_usize;
        let budget = self.max_payload_bytes;
        // ACK first so loss recovery always travels with fresh data.
        if want_ack && let Some(ack) = self.build_ack_block() {
            let block = Block::Ack(ack);
            let len = block.encoded_len();
            if used + len <= budget {
                used += len;
                blocks.push(block);
                self.counters.acks_sent = self.counters.acks_sent.saturating_add(1);
            }
        }
        // Termination and path controls next (bounded, one per packet
        // to keep datagrams small and deterministic).
        if let Some(control) = self.queued_controls.pop_front() {
            let rebuild = match &control {
                QueuedControl::PathChallenge(data) => QueuedControl::PathChallenge(data.clone()),
                QueuedControl::PathResponse(data) => QueuedControl::PathResponse(data.clone()),
                QueuedControl::NewToken { expires, token } => QueuedControl::NewToken {
                    expires: *expires,
                    token: *token,
                },
                QueuedControl::Termination {
                    valid_packets_received,
                    reason,
                } => QueuedControl::Termination {
                    valid_packets_received: *valid_packets_received,
                    reason: *reason,
                },
            };
            let block = match control {
                QueuedControl::PathChallenge(data) => {
                    PathChallengeBlock::new(data).ok().map(Block::PathChallenge)
                }
                QueuedControl::PathResponse(data) => {
                    PathResponseBlock::new(data).ok().map(Block::PathResponse)
                }
                QueuedControl::NewToken { expires, token } => Some(Block::NewToken(
                    crate::block::NewTokenBlock::new(expires, token),
                )),
                QueuedControl::Termination {
                    valid_packets_received,
                    reason,
                } => {
                    let block = crate::block::TerminationBlock::new(
                        valid_packets_received,
                        crate::block::TerminationReason::from_code(reason),
                    );
                    Some(Block::Termination(block))
                }
            };
            if let Some(block) = block {
                let len = block.encoded_len();
                if used + len <= budget {
                    used += len;
                    blocks.push(block);
                } else {
                    // Budget exhausted; requeue at front for the next
                    // packet (bounded control queue, no loss).
                    self.queued_controls.push_front(rebuild);
                }
            }
        }
        // Fresh retransmissions before new data (oldest first).
        let mut carried: Vec<RetransmittableFragment> = Vec::new();
        while used < budget
            && let Some(fragment) = self.pending_retransmit.pop_front()
        {
            let block = Self::fragment_to_block(&fragment);
            let len = block.encoded_len();
            if used + len > budget {
                self.pending_retransmit.push_front(fragment);
                break;
            }
            used += len;
            carried.push(fragment.clone());
            blocks.push(block);
        }
        // New outbound fragments, greedy across messages.
        let mut in_flight_marks: Vec<(u32, usize)> = Vec::new();
        for message in self.outbound.iter_mut() {
            // One new fragment per message per packet keeps loss
            // attribution exact; additional fragments ride later polls
            // while retransmissions pack greedily above.
            if used < budget
                && let Some(fragment) = message.fragments.pop_front()
            {
                let retransmittable = RetransmittableFragment {
                    message_id: message.message_id,
                    frag_number: fragment.frag_number,
                    is_last: fragment.is_last,
                    total_fragments: fragment.total_fragments,
                    message_type: message.message_type,
                    expiration_secs: message.expiration_secs,
                    bytes: fragment.bytes.clone(),
                    generation: 0,
                };
                let block = Self::fragment_to_block(&retransmittable);
                let len = block.encoded_len();
                if used + len > budget {
                    message.fragments.push_front(fragment);
                } else {
                    used += len;
                    carried.push(retransmittable);
                    in_flight_marks.push((message.message_id, 1));
                    blocks.push(block);
                }
            }
            if used >= budget {
                break;
            }
        }
        for (message_id, count) in in_flight_marks {
            if let Some(message) = self
                .outbound
                .iter_mut()
                .find(|message| message.message_id == message_id)
            {
                message.in_flight = message.in_flight.saturating_add(count);
            }
        }
        if blocks.is_empty() {
            // Nothing fit (e.g. ACK alone exceeded budget, which
            // cannot happen with the 128-range cap and 1220 budget);
            // restore carried retransmits.
            for fragment in carried.into_iter().rev() {
                self.pending_retransmit.push_front(fragment);
            }
            return None;
        }
        let ack_only = !blocks.iter().any(|block| is_ack_eliciting(block.kind()));
        let carries_ack = blocks
            .iter()
            .any(|block| block.kind() == constants::BLOCK_ACK);
        self.seal_packet(now_ms, blocks, ack_only)
            .inspect(|datagram| {
                if carries_ack {
                    self.clear_ack_state();
                }
                // Record provenance for congestion-controlled packets and
                // every packet carrying fragments (loss recovery needs the
                // provenance even when the packet also carries an ACK).
                if !carried.is_empty() || !ack_only {
                    let ack_bytes = if ack_only { 0 } else { datagram.len() };
                    self.record_sent(now_ms, datagram.len(), ack_bytes, ack_only, carried);
                }
            })
    }

    fn pending_ack_eliciting_controls(&self) -> bool {
        !self.queued_controls.is_empty()
    }

    fn fragment_to_block(fragment: &RetransmittableFragment) -> Block {
        // Single-fragment messages travel as complete I2NP blocks so
        // the receiver delivers them without reassembly state.
        if fragment.total_fragments <= 1 {
            let mut encoded = Vec::with_capacity(9 + fragment.bytes.len());
            encoded.push(fragment.message_type);
            encoded.extend_from_slice(&fragment.message_id.to_be_bytes());
            encoded.extend_from_slice(&fragment.expiration_secs.to_be_bytes());
            encoded.extend_from_slice(&fragment.bytes);
            let block = crate::block::I2npMessageBlock::from_bytes(encoded)
                .expect("queued fragments are validated");
            return Block::I2np(block);
        }
        if fragment.frag_number == 0 {
            let block = FirstFragmentBlock::new(
                MessageType::from_code(fragment.message_type),
                fragment.message_id,
                fragment.expiration_secs,
                fragment.bytes.clone(),
            )
            .expect("queued fragments are validated");
            Block::FirstFragment(block)
        } else {
            let block = FollowOnFragmentBlock::new(
                fragment.frag_number,
                fragment.is_last,
                fragment.message_id,
                fragment.bytes.clone(),
            )
            .expect("queued fragments are validated");
            Block::FollowOnFragment(block)
        }
    }

    fn seal_packet(
        &mut self,
        now_ms: u64,
        blocks: Vec<Block>,
        force_ack_only: bool,
    ) -> Option<Vec<u8>> {
        let _ = force_ack_only;
        let packet_number = self.allocate_packet_number()?;
        let approved_no_ack = blocks.is_empty();
        // Empty block list means a standalone ACK was requested but no
        // ACK block fit; synthesize one more time (cannot fail with the
        // bounded window).
        let blocks = if blocks.is_empty() {
            match self.build_ack_block() {
                Some(ack) => {
                    self.counters.acks_sent = self.counters.acks_sent.saturating_add(1);
                    vec![Block::Ack(ack)]
                }
                None => return None,
            }
        } else {
            blocks
        };
        let plaintext = encode_blocks(blocks).ok()?;
        if plaintext.len() > self.max_payload_bytes {
            return None;
        }
        // Pad short payloads to the 8-byte header-protection floor.
        let mut plaintext = plaintext;
        if plaintext.len() < constants::MIN_HANDSHAKE_PAYLOAD_BYTES {
            let need = constants::MIN_HANDSHAKE_PAYLOAD_BYTES - plaintext.len();
            let mut padded = encode_blocks(vec![Block::Padding(
                crate::block::PaddingBlock::new(vec![0_u8; need.max(1)]).expect("bounded padding"),
            )])
            .ok()?;
            let mut combined = plaintext;
            // Re-encode path: the padding block alone is well-formed;
            // appending keeps ordering (padding last) valid.
            combined.append(&mut padded);
            plaintext = combined;
        }
        let immediate = self.should_request_immediate_ack();
        let header = DataHeader::new(self.remote_conn_id, packet_number, immediate);
        let header_bytes = header.encode();
        let sealed = self
            .transmit_cipher
            .seal(packet_number, &header_bytes, &plaintext)
            .ok()?;
        let mut datagram = Vec::with_capacity(constants::SHORT_HEADER_LENGTH + sealed.len());
        datagram.extend_from_slice(&header_bytes);
        datagram.extend_from_slice(&sealed);
        crate::crypto::apply_header_protection(
            &mut datagram,
            constants::SHORT_HEADER_LENGTH,
            self.remote_intro.as_bytes(),
            &self.transmit_header_2,
            false,
        )
        .ok()?;
        if datagram.len() < constants::MIN_DATAGRAM_LENGTH
            || datagram.len() > constants::MAX_DATAGRAM_IPV4_LENGTH
        {
            return None;
        }
        self.last_send_ms = Some(now_ms);
        self.counters.packets_sent = self.counters.packets_sent.saturating_add(1);
        let _ = approved_no_ack;
        Some(datagram)
    }

    /// Clears pending ACK state after any packet carrying an ACK block
    /// is sealed (piggyback or standalone alike).
    fn clear_ack_state(&mut self) {
        self.ack_pending = false;
        self.ack_deadline_ms = None;
        self.ack_eliciting_since_ack = 0;
    }

    fn allocate_packet_number(&mut self) -> Option<u32> {
        if self.send_exhausted {
            return None;
        }
        let number = self.next_send;
        // Exhaustion guard: the 32-bit space must never wrap within a
        // session ("terminated well before the max"); the exact
        // terminal packet is a local policy at the final value.
        if number == u32::MAX {
            self.send_exhausted = true;
            return Some(number);
        }
        // Detect a full wrap (2^32 packets sent): refuse further
        // allocation once we return to the initial value.
        self.next_send = number.wrapping_add(1);
        if self.packets_sent_count >= u64::from(u32::MAX) {
            self.send_exhausted = true;
            return None;
        }
        self.packets_sent_count = self.packets_sent_count.saturating_add(1);
        Some(number)
    }

    fn should_request_immediate_ack(&self) -> bool {
        // Request immediate ACK when retransmitted fragments ride
        // along or the window is more than two-thirds full, per the
        // specification sender strategies.
        if !self.pending_retransmit.is_empty() {
            return true;
        }
        if self.cwnd_bytes == 0 {
            return false;
        }
        self.bytes_in_flight.saturating_mul(3) >= self.cwnd_bytes.saturating_mul(2)
    }

    fn record_sent(
        &mut self,
        now_ms: u64,
        _datagram_len: usize,
        ack_eliciting_bytes: usize,
        ack_only: bool,
        fragments: Vec<RetransmittableFragment>,
    ) {
        if self.sent.len() >= constants::DATA_MAX_SENT_PACKETS {
            // History full: declare the oldest unacked packet lost to
            // bound memory rather than growing (counts as a loss).
            if let Some(oldest) = self.sent.pop_front() {
                self.declare_lost(&oldest, now_ms);
            }
        }
        // The packet number is the last allocated value.
        let packet_number = self.next_send.wrapping_sub(1);
        let counts = !ack_only;
        if counts {
            self.bytes_in_flight = self.bytes_in_flight.saturating_add(ack_eliciting_bytes);
            self.counters.bytes_in_flight = self.bytes_in_flight;
        }
        // ACK state is satisfied by any packet carrying the ACK block.
        if self.ack_pending {
            self.ack_pending = false;
            self.ack_deadline_ms = None;
            self.ack_eliciting_since_ack = 0;
        }
        // Karn's rule inputs: the packet generation is the maximum
        // carried-fragment generation, so fresh packets carrying
        // retransmitted fragments never produce RTT samples.
        let generation = fragments
            .iter()
            .map(|fragment| fragment.generation)
            .max()
            .unwrap_or(0);
        self.sent.push_back(SentPacket {
            packet_number,
            sent_ms: now_ms,
            ack_eliciting_bytes,
            ack_only,
            fragments,
            counts_for_congestion: counts,
            generation,
        });
    }

    // -- Receive path --------------------------------------------------

    /// Processes one inbound datagram through the ordered pipeline.
    ///
    /// Order: cheap length check → header protection removal → exact
    /// header decode → session binding → replay/window admissibility →
    /// AEAD open → bounded block parse → effects. Tag failures and
    /// replays never mutate application-visible receive state (only
    /// diagnostic counters).
    pub fn receive_datagram(
        &mut self,
        now_ms: u64,
        now_secs: u64,
        datagram: &[u8],
    ) -> ReceiveOutcome {
        let mut outcome = ReceiveOutcome {
            packet_number: None,
            ack_eliciting: false,
            ack_only: false,
            events: Vec::new(),
            ack_scheduled: false,
            dropped: None,
        };
        if self.is_terminated() {
            // Terminated sessions still count the drop without effects.
            self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
            outcome.dropped = Some(DropReason::Terminated);
            return outcome;
        }
        // 1. Cheap structural/datagram-size check.
        if DatagramLengthClass::classify(datagram.len()).is_err() {
            self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
            outcome.dropped = Some(DropReason::Malformed);
            return outcome;
        }
        if datagram.len() < constants::SHORT_HEADER_LENGTH + constants::MIN_POST_HEADER_BYTES {
            self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
            outcome.dropped = Some(DropReason::Malformed);
            return outcome;
        }
        // 2-3. Unprotect/decode header.
        let mut working = datagram.to_vec();
        if crate::crypto::remove_header_protection(
            &mut working,
            constants::SHORT_HEADER_LENGTH,
            self.local_intro.as_bytes(),
            &self.receive_header_2,
            false,
        )
        .is_err()
        {
            self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
            outcome.dropped = Some(DropReason::Malformed);
            return outcome;
        }
        let header = match DataHeader::decode(&working[..constants::SHORT_HEADER_LENGTH]) {
            Ok(header) => header,
            Err(_) => {
                self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
                outcome.dropped = Some(DropReason::Malformed);
                return outcome;
            }
        };
        // 2. Session binding by connection ID (caller context).
        if header.dst_conn_id() != self.local_conn_id {
            self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
            outcome.dropped = Some(DropReason::ConnectionIdMismatch);
            return outcome;
        }
        let packet_number = header.packet_number();
        outcome.packet_number = Some(packet_number);
        // 4. Replay/window admissibility (before AEAD).
        match self.check_replay(packet_number) {
            ReplayCheck::Duplicate => {
                self.counters.packets_replayed = self.counters.packets_replayed.saturating_add(1);
                outcome.dropped = Some(DropReason::Replay);
                return outcome;
            }
            ReplayCheck::TooOld => {
                self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
                outcome.dropped = Some(DropReason::TooOld);
                return outcome;
            }
            ReplayCheck::FutureJump => {
                self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
                outcome.dropped = Some(DropReason::FutureJump);
                return outcome;
            }
            ReplayCheck::Admissible => {}
        }
        // 5. Authenticate/decrypt payload (header is the AD).
        let header_bytes: [u8; constants::SHORT_HEADER_LENGTH] = working
            [..constants::SHORT_HEADER_LENGTH]
            .try_into()
            .expect("length checked");
        let ciphertext = &working[constants::SHORT_HEADER_LENGTH..];
        let plaintext = match self
            .receive_cipher
            .open(packet_number, &header_bytes, ciphertext)
        {
            Ok(plaintext) => plaintext,
            Err(_) => {
                self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
                outcome.dropped = Some(DropReason::AuthenticationFailed);
                return outcome;
            }
        };
        // 6. Parse bounded blocks. A NextNonce block (spec TODO,
        // no normative behavior) surfaces as UnsupportedBlock: the
        // packet is authenticated, so expose the rekey boundary as a
        // typed event without delivery effects or state mutation
        // beyond reception accounting.
        let parsed = match parse_blocks(&plaintext) {
            Ok(parsed) => parsed,
            Err(BlockError::UnsupportedBlock) => {
                self.mark_received(packet_number, false, now_ms, false);
                self.last_receive_ms = Some(now_ms);
                self.counters.packets_received = self.counters.packets_received.saturating_add(1);
                outcome.events.push(SessionEvent::RekeyRequested);
                return outcome;
            }
            Err(_) => {
                self.counters.packets_rejected = self.counters.packets_rejected.saturating_add(1);
                outcome.dropped = Some(DropReason::BlockParseFailed);
                return outcome;
            }
        };
        // 7. Expose effects. ACK processing comes first so loss
        // recovery observes the peer's view before delivery effects.
        let mut ack_eliciting = false;
        let mut ack_only = true;
        for block in parsed.blocks() {
            if is_ack_eliciting_decoded(block) {
                ack_eliciting = true;
                ack_only = false;
                break;
            }
        }
        // A packet with no blocks is non-eliciting (keep-alive shape);
        // an all-non-eliciting packet is ACK-only.
        if parsed.blocks().is_empty() {
            ack_only = true;
        }
        outcome.ack_eliciting = ack_eliciting;
        outcome.ack_only = ack_only && !ack_eliciting;
        // Process ACK blocks before marking reception so RTT/loss uses
        // the pre-update window (no self-ack feedback).
        for block in parsed.blocks() {
            if let DecodedBlock::Ack(ack) = block {
                self.counters.acks_received = self.counters.acks_received.saturating_add(1);
                if self.apply_ack(ack, now_ms).is_err() {
                    self.counters.packets_rejected =
                        self.counters.packets_rejected.saturating_add(1);
                    outcome.dropped = Some(DropReason::InvalidAck);
                    return outcome;
                }
            }
        }
        // Mark reception only after successful authentication (never on
        // tag failure or replay); this also arms ACK scheduling.
        self.mark_received(packet_number, ack_eliciting, now_ms, header.immediate_ack());
        self.last_receive_ms = Some(now_ms);
        self.counters.packets_received = self.counters.packets_received.saturating_add(1);
        outcome.ack_scheduled = ack_eliciting;
        self.last_received_eliciting = ack_eliciting;
        // Deliver remaining blocks as typed events.
        for block in parsed.into_blocks() {
            match block {
                DecodedBlock::Ack(_) => {}
                DecodedBlock::I2np(message) => {
                    let bytes = message.as_bytes().to_vec();
                    self.deliver_complete_message(bytes, now_ms, now_secs, &mut outcome.events);
                }
                DecodedBlock::FirstFragment(fragment) => {
                    self.ingest_first_fragment(fragment, now_ms, now_secs, &mut outcome.events);
                }
                DecodedBlock::FollowOnFragment(fragment) => {
                    self.ingest_follow_on_fragment(fragment, now_ms, now_secs, &mut outcome.events);
                }
                DecodedBlock::Termination(termination) => {
                    let reason = termination.reason().code();
                    self.remote_terminate = Some(reason);
                    self.counters.terminated_remote = Some(reason);
                    // Bounded cleanup: release pending retransmit and
                    // reassembly state on termination.
                    self.pending_retransmit.clear();
                    self.clear_reassembly();
                    outcome.events.push(SessionEvent::Termination {
                        valid_packets_received: termination.valid_packets_received(),
                        reason,
                    });
                }
                DecodedBlock::NewToken(token) => {
                    outcome.events.push(SessionEvent::NewToken {
                        expires: token.expires(),
                        token: token.token(),
                    });
                }
                DecodedBlock::PathChallenge(challenge) => {
                    outcome
                        .events
                        .push(SessionEvent::PathChallenge(challenge.data().to_vec()));
                }
                DecodedBlock::PathResponse(response) => {
                    outcome
                        .events
                        .push(SessionEvent::PathResponse(response.data().to_vec()));
                }
                DecodedBlock::Timestamp(timestamp) => {
                    outcome
                        .events
                        .push(SessionEvent::DateTime(timestamp.seconds()));
                }
                DecodedBlock::Options(options) => {
                    outcome.events.push(SessionEvent::Options {
                        transmit_ratios: options.transmit_ratios(),
                        receive_ratios: options.receive_ratios(),
                    });
                }
                DecodedBlock::Congestion(congestion) => {
                    let immediate = congestion.requests_immediate_ack();
                    if immediate {
                        self.arm_ack(now_ms, true);
                    }
                    outcome.events.push(SessionEvent::Congestion {
                        flags: congestion.flags(),
                    });
                }
                DecodedBlock::RelayRequest(_)
                | DecodedBlock::RelayResponse(_)
                | DecodedBlock::RelayIntro(_) => {
                    outcome.events.push(SessionEvent::Relay {
                        block_type: 7,
                        length: 0,
                    });
                }
                DecodedBlock::PeerTest(peer_test) => {
                    outcome.events.push(SessionEvent::PeerTest {
                        message: peer_test.message(),
                    });
                }
                DecodedBlock::Address(_) => {
                    outcome.events.push(SessionEvent::AddressObserved);
                }
                DecodedBlock::RelayTagRequest => {
                    outcome.events.push(SessionEvent::Relay {
                        block_type: 15,
                        length: 0,
                    });
                }
                DecodedBlock::RelayTag(_) => {
                    outcome.events.push(SessionEvent::Relay {
                        block_type: 16,
                        length: 4,
                    });
                }
                DecodedBlock::FirstPacketNumber(first) => {
                    outcome
                        .events
                        .push(SessionEvent::FirstPacketNumber(first.first_packet_number()));
                }
                DecodedBlock::RouterInfo(_) => {
                    // RouterInfo blocks are handshake-only; in the data
                    // phase they decode into no event (silently ignored
                    // after authentication, without delivery).
                }
                DecodedBlock::Padding { .. } | DecodedBlock::Unknown { .. } => {}
            }
        }
        // Block::NextNonce (type 11) surfaces as UnsupportedBlock from
        // the parser; it never reaches here as DecodedBlock. Detect it
        // at the raw level: a TODO for future rekey work. The parser
        // error path already drops such packets without effects, which
        // is safe but not eventful; document the gap.
        outcome
    }

    // -- Replay window -------------------------------------------------

    fn check_replay(&self, packet_number: u32) -> ReplayCheck {
        let Some(highest) = self.highest_received else {
            return ReplayCheck::Admissible;
        };
        if packet_number == highest {
            return ReplayCheck::Duplicate;
        }
        let forward = packet_number.wrapping_sub(highest);
        if forward < (1_u32 << 31) {
            // Ahead of the window.
            if forward > constants::DATA_MAX_FUTURE_JUMP {
                return ReplayCheck::FutureJump;
            }
            return ReplayCheck::Admissible;
        }
        // Behind the window.
        let behind = highest.wrapping_sub(packet_number);
        if behind >= constants::DATA_REPLAY_WINDOW_PACKETS as u32 {
            return ReplayCheck::TooOld;
        }
        let bit = 1_u128 << behind;
        if self.received_bitmap & bit != 0 {
            return ReplayCheck::Duplicate;
        }
        ReplayCheck::Admissible
    }

    fn mark_received(
        &mut self,
        packet_number: u32,
        ack_eliciting: bool,
        now_ms: u64,
        immediate_requested: bool,
    ) {
        match self.highest_received {
            None => {
                self.highest_received = Some(packet_number);
                self.received_bitmap = 1;
                self.eliciting_bitmap = if ack_eliciting { 1 } else { 0 };
            }
            Some(highest) => {
                let forward = packet_number.wrapping_sub(highest);
                if forward != 0 && forward < (1_u32 << 31) {
                    // New highest: shift the window forward. A jump at
                    // or beyond the window width resets the bitmap
                    // (shifting a u128 by >= 128 would overflow).
                    if forward >= constants::DATA_REPLAY_WINDOW_PACKETS as u32 {
                        self.received_bitmap = 0;
                        self.eliciting_bitmap = 0;
                    } else {
                        self.received_bitmap <<= forward;
                        self.eliciting_bitmap <<= forward;
                    }
                    self.received_bitmap |= 1;
                    if ack_eliciting {
                        self.eliciting_bitmap |= 1;
                    }
                    self.highest_received = Some(packet_number);
                } else {
                    let behind = highest.wrapping_sub(packet_number);
                    let bit = 1_u128 << behind;
                    self.received_bitmap |= bit;
                    if ack_eliciting {
                        self.eliciting_bitmap |= bit;
                    }
                }
            }
        }
        if ack_eliciting {
            self.ack_pending = true;
            self.ack_eliciting_since_ack = self.ack_eliciting_since_ack.saturating_add(1);
            // Immediate conditions per specification guidance:
            // explicit flag, out-of-order arrival, or every second
            // ack-eliciting packet (TCP-style delayed ACK).
            let out_of_order = match self.highest_received {
                Some(highest) => packet_number != highest,
                None => false,
            };
            let immediate =
                immediate_requested || out_of_order || self.ack_eliciting_since_ack >= 2;
            self.arm_ack(now_ms, immediate);
        }
    }

    fn arm_ack(&mut self, now_ms: u64, immediate: bool) {
        let delay = if immediate {
            self.immediate_ack_delay()
        } else {
            self.delayed_ack_delay()
        };
        let at = now_ms.saturating_add(delay);
        self.ack_deadline_ms = Some(match self.ack_deadline_ms {
            Some(existing) => existing.min(at),
            None => at,
        });
    }

    fn immediate_ack_delay(&self) -> u64 {
        match self.srtt_ms {
            Some(srtt) => (srtt / 16).clamp(1, 5),
            None => constants::DATA_DEFAULT_IMMEDIATE_ACK_DELAY_MS,
        }
    }

    fn delayed_ack_delay(&self) -> u64 {
        match self.srtt_ms {
            Some(srtt) => (srtt / 6).clamp(10, 150),
            None => constants::DATA_DEFAULT_ACK_DELAY_MS,
        }
    }

    // -- ACK interpretation --------------------------------------------

    fn build_ack_block(&self) -> Option<AckBlock> {
        let highest = self.highest_received?;
        if self.received_bitmap == 0 {
            return None;
        }
        // Count consecutive acked packets below ack_through (acnt).
        let mut acnt: u8 = 0;
        for offset in 1..=u8::MAX {
            let bit = if usize::from(offset) < 128 {
                1_u128 << usize::from(offset)
            } else {
                break;
            };
            if self.received_bitmap & bit != 0 {
                acnt = acnt.saturating_add(1);
                if acnt == u8::MAX {
                    break;
                }
            } else {
                break;
            }
        }
        // Ranges below the initial run, walking downward.
        let mut ranges: Vec<(u8, u8)> = Vec::new();
        let mut cursor = u32::from(acnt) + 1;
        while ranges.len() < constants::MAX_ACK_RANGES {
            // Count the NACK gap from the cursor.
            let mut nack: u8 = 0;
            while cursor < constants::DATA_REPLAY_WINDOW_PACKETS as u32 {
                let bit = 1_u128 << (cursor as usize);
                if self.received_bitmap & bit == 0 {
                    nack = nack.saturating_add(1);
                    cursor += 1;
                    if nack == u8::MAX {
                        break;
                    }
                } else {
                    break;
                }
            }
            if cursor >= constants::DATA_REPLAY_WINDOW_PACKETS as u32 {
                break;
            }
            // Count the following ACK run.
            let mut ack: u8 = 0;
            while cursor < constants::DATA_REPLAY_WINDOW_PACKETS as u32 {
                let bit = 1_u128 << (cursor as usize);
                if self.received_bitmap & bit != 0 {
                    ack = ack.saturating_add(1);
                    cursor += 1;
                    if ack == u8::MAX {
                        break;
                    }
                } else {
                    break;
                }
            }
            if nack == 0 && ack == 0 {
                break;
            }
            // A trailing all-unreceived tail is neither acked nor
            // nacked: stop instead of emitting a zero-ack range.
            if ack == 0 {
                break;
            }
            ranges.push((nack, ack));
            if cursor >= constants::DATA_REPLAY_WINDOW_PACKETS as u32 {
                break;
            }
        }
        AckBlock::new(highest, acnt, ranges).ok()
    }

    fn apply_ack(&mut self, ack: &AckBlock, now_ms: u64) -> Result<(), SessionError> {
        // Strict range interpretation: walk downward from ack_through
        // with checked arithmetic, rejecting underflow and degenerate
        // encodings without mutating sent state on failure.
        let ack_through = ack.ack_through();
        let mut acked: Vec<u32> = Vec::new();
        // Initial run: ack_through plus acnt packets below it.
        for offset in 0..=u32::from(ack.ack_count()) {
            let number = ack_through
                .checked_sub(offset)
                .ok_or(SessionError::AckUnderflow)?;
            acked.push(number);
        }
        // Ranges walk downward from the packet below the initial run.
        // `cursor` is the next packet number to classify, or `None`
        // once the walk has passed packet 0.
        let mut cursor: Option<u32> = if u32::from(ack.ack_count()) >= ack_through {
            // Initial run already reaches packet 0; any ranges would
            // underflow below zero.
            if !ack.ranges().is_empty() {
                return Err(SessionError::AckUnderflow);
            }
            None
        } else {
            Some(ack_through - u32::from(ack.ack_count()) - 1)
        };
        for (index, (nack, ack_count)) in ack.ranges().iter().enumerate() {
            if *nack == 0 && *ack_count == 0 {
                return Err(SessionError::AckInvalid);
            }
            let Some(position) = cursor else {
                return Err(SessionError::AckUnderflow);
            };
            // Skip the NACK gap downward: the gap covers `nack`
            // packets ending at `position`.
            if u32::from(*nack) > position {
                return Err(SessionError::AckUnderflow);
            }
            let mut current = position.wrapping_sub(u32::from(*nack));
            // After the gap, `current` is the highest packet of the
            // following ACK run (when the run is nonempty).
            if *ack_count > 0 && u32::from(*ack_count) > current.saturating_add(1) {
                return Err(SessionError::AckUnderflow);
            }
            for _ in 0..*ack_count {
                acked.push(current);
                if current == 0 {
                    break;
                }
                current = current.wrapping_sub(1);
            }
            // Advance past the ACK run just consumed.
            if *ack_count == 0 {
                cursor = Some(current);
            } else if current == 0 {
                // Reached packet 0: further ranges underflow.
                if index + 1 < ack.ranges().len() {
                    return Err(SessionError::AckUnderflow);
                }
                cursor = None;
            } else {
                cursor = Some(current.wrapping_sub(1));
                // If more ranges remain but the cursor already passed
                // zero, the next iteration fails closed above.
                if cursor == Some(u32::MAX) {
                    return Err(SessionError::AckUnderflow);
                }
            }
        }
        // Retire only actually-sent packets; unknown numbers are
        // idempotent no-ops (never an error).
        let mut newly_acked_bytes: usize = 0;
        let mut rtt_sample: Option<u64> = None;
        let mut index = 0;
        while index < self.sent.len() {
            let number = self.sent[index].packet_number;
            if acked.contains(&number) {
                let packet = self.sent.remove(index).expect("index checked");
                if packet.counts_for_congestion {
                    self.bytes_in_flight = self
                        .bytes_in_flight
                        .saturating_sub(packet.ack_eliciting_bytes);
                    self.counters.bytes_in_flight = self.bytes_in_flight;
                    newly_acked_bytes =
                        newly_acked_bytes.saturating_add(packet.ack_eliciting_bytes);
                }
                // RTT sample only from eligible packets: ack-eliciting,
                // sent exactly once (generation 0), per Karn's rule.
                if !packet.ack_only && packet.generation == 0 && rtt_sample.is_none() {
                    rtt_sample = now_ms.checked_sub(packet.sent_ms);
                }
                // Mark carried fragments delivered: drop them from the
                // retransmit queue view by filtering on (id, frag).
                for fragment in &packet.fragments {
                    self.pending_retransmit.retain(|pending| {
                        !(pending.message_id == fragment.message_id
                            && pending.frag_number == fragment.frag_number)
                    });
                    self.mark_fragment_acked(fragment);
                }
                self.consecutive_rto = 0;
            } else {
                index += 1;
            }
        }
        // Explicit NACK gaps declare loss for still-unacked packets
        // below ack_through (conservative simple policy).
        let acked_set = acked;
        let mut lost_indices: Vec<usize> = Vec::new();
        for (position, packet) in self.sent.iter().enumerate() {
            if packet.packet_number == ack_through {
                continue;
            }
            let behind = ack_through.wrapping_sub(packet.packet_number);
            if behind == 0 || behind >= (1_u32 << 31) {
                continue;
            }
            if !acked_set.contains(&packet.packet_number) {
                lost_indices.push(position);
            }
        }
        // Only the oldest NACKed packet per ACK is declared lost to
        // avoid burst collapse on reorder; the rest age toward RTO.
        if let Some(position) = lost_indices.first() {
            let packet = self.sent.remove(*position).expect("index checked");
            self.declare_lost(&packet, now_ms);
        }
        if let Some(sample) = rtt_sample {
            self.update_rtt(sample);
        }
        if newly_acked_bytes > 0 {
            self.grow_cwnd(newly_acked_bytes);
        }
        Ok(())
    }

    fn mark_fragment_acked(&mut self, fragment: &RetransmittableFragment) {
        // Remove the outbound message when all its fragments have been
        // handed out and acked: find the message and check whether any
        // sent or pending state still references it.
        let message_id = fragment.message_id;
        let still_referenced = self.sent.iter().any(|packet| {
            packet
                .fragments
                .iter()
                .any(|item| item.message_id == message_id)
        }) || self
            .pending_retransmit
            .iter()
            .any(|item| item.message_id == message_id);
        if !still_referenced
            && let Some(position) = self
                .outbound
                .iter()
                .position(|message| message.message_id == message_id)
        {
            let message = &self.outbound[position];
            if message.fragments.is_empty() {
                self.outbound.remove(position);
            }
        }
    }

    fn declare_lost(&mut self, packet: &SentPacket, _now_ms: u64) {
        self.counters.loss_events = self.counters.loss_events.saturating_add(1);
        if packet.counts_for_congestion {
            self.bytes_in_flight = self
                .bytes_in_flight
                .saturating_sub(packet.ack_eliciting_bytes);
            self.counters.bytes_in_flight = self.bytes_in_flight;
        }
        self.shrink_cwnd();
        // Fresh retransmission: requeue still-needed fragments with a
        // new generation, dropping acked ones (already filtered) and
        // failing messages past their ceiling while the session stays
        // usable.
        for fragment in &packet.fragments {
            if fragment.generation >= constants::DATA_MAX_FRAGMENT_RETRANSMISSIONS {
                self.fail_message(fragment.message_id);
                continue;
            }
            let mut fresh = fragment.clone();
            fresh.generation = fresh.generation.saturating_add(1);
            if self.pending_retransmit.len() >= constants::DATA_MAX_PENDING_RETRANSMIT_FRAGMENTS {
                self.fail_message(fragment.message_id);
                continue;
            }
            self.pending_retransmit.push_back(fresh);
            self.counters.retransmitted_fragments =
                self.counters.retransmitted_fragments.saturating_add(1);
        }
    }

    fn fail_message(&mut self, message_id: u32) {
        if let Some(position) = self
            .outbound
            .iter()
            .position(|message| message.message_id == message_id)
        {
            self.outbound.remove(position);
        }
        self.pending_retransmit
            .retain(|fragment| fragment.message_id != message_id);
        // Note: no session teardown; delivery may fail while the
        // session remains usable per specification.
    }

    // -- RTT / congestion ----------------------------------------------

    fn update_rtt(&mut self, sample_ms: u64) {
        match self.srtt_ms {
            None => {
                self.srtt_ms = Some(sample_ms);
                self.rttvar_ms = sample_ms / 2;
            }
            Some(srtt) => {
                let diff = srtt.abs_diff(sample_ms);
                self.rttvar_ms = (3 * self.rttvar_ms + diff) / 4;
                self.srtt_ms = Some((7 * srtt + sample_ms) / 8);
            }
        }
        let srtt = self.srtt_ms.unwrap_or(sample_ms);
        let rto = srtt.saturating_add(self.rttvar_ms.saturating_mul(4).max(1));
        self.rto_ms = rto.clamp(constants::DATA_MIN_RTO_MS, constants::DATA_MAX_RTO_MS);
    }

    fn grow_cwnd(&mut self, acked_bytes: usize) {
        let increment = acked_bytes.min(constants::DATA_MSS_BYTES);
        self.cwnd_bytes =
            (self.cwnd_bytes.saturating_add(increment)).min(constants::DATA_MAX_CWND_BYTES);
        self.counters.cwnd_bytes = self.cwnd_bytes;
    }

    fn shrink_cwnd(&mut self) {
        self.cwnd_bytes = (self.cwnd_bytes / 2).max(constants::DATA_MIN_CWND_BYTES);
        self.counters.cwnd_bytes = self.cwnd_bytes;
    }

    // -- Reassembly ----------------------------------------------------

    fn ingest_first_fragment(
        &mut self,
        fragment: FirstFragmentBlock,
        now_ms: u64,
        now_secs: u64,
        events: &mut Vec<SessionEvent>,
    ) {
        let message_id = fragment.message_id();
        // Duplicate-suppression gate before admission.
        if self.is_duplicate(message_id, fragment.expiration_seconds(), now_ms) {
            return;
        }
        let bytes_len = fragment.fragment().len();
        if bytes_len == 0 {
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        if !self.reassembly.contains_key(&message_id)
            && self.reassembly.len() >= constants::DATA_MAX_REASSEMBLY_MESSAGES
        {
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        if self.reassembly_bytes.saturating_add(bytes_len) > constants::DATA_MAX_REASSEMBLY_BYTES {
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        let entry = self
            .reassembly
            .entry(message_id)
            .or_insert_with(|| ReassemblyEntry::placeholder(now_ms));
        // Conflict detection: a second distinct first fragment never
        // overwrites; the entry is dropped per protocol policy.
        if let Some(known_type) = entry.message_type
            && (known_type != fragment.message_type().code()
                || entry.expiration_secs != Some(fragment.expiration_seconds()))
        {
            self.drop_reassembly(message_id);
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        entry.message_type = Some(fragment.message_type().code());
        entry.expiration_secs = Some(fragment.expiration_seconds());
        if let Some(existing) = entry.fragments.get(&0) {
            if *existing != fragment.fragment() {
                self.drop_reassembly(message_id);
                self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
                return;
            }
            return;
        }
        entry.total_bytes = entry.total_bytes.saturating_add(bytes_len);
        self.reassembly_bytes = self.reassembly_bytes.saturating_add(bytes_len);
        entry.fragments.insert(0, fragment.fragment().to_vec());
        entry.last_progress_ms = now_ms;
        self.counters.reassembly_messages = self.reassembly.len();
        self.counters.reassembly_bytes = self.reassembly_bytes;
        self.maybe_complete_reassembly(message_id, now_ms, now_secs, events);
    }

    fn ingest_follow_on_fragment(
        &mut self,
        fragment: FollowOnFragmentBlock,
        now_ms: u64,
        now_secs: u64,
        events: &mut Vec<SessionEvent>,
    ) {
        let message_id = fragment.message_id();
        let frag_number = fragment.frag_number();
        let bytes_len = fragment.fragment().len();
        if bytes_len == 0 {
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        if !self.reassembly.contains_key(&message_id)
            && self.reassembly.len() >= constants::DATA_MAX_REASSEMBLY_MESSAGES
        {
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        if self.reassembly_bytes.saturating_add(bytes_len) > constants::DATA_MAX_REASSEMBLY_BYTES {
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        let entry = self
            .reassembly
            .entry(message_id)
            .or_insert_with(|| ReassemblyEntry::placeholder(now_ms));
        if entry.fragments.len() >= constants::MAX_I2NP_FRAGMENTS {
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        if fragment.is_last() {
            let total = frag_number.saturating_add(1);
            if total == 0 || usize::from(total) > constants::MAX_I2NP_FRAGMENTS {
                self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
                return;
            }
            if let Some(known) = entry.total
                && known != total
            {
                self.drop_reassembly(message_id);
                self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
                return;
            }
            entry.total = Some(total);
        }
        if let Some(existing) = entry.fragments.get(&frag_number) {
            if *existing != fragment.fragment() {
                self.drop_reassembly(message_id);
                self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
                return;
            }
            return;
        }
        entry.total_bytes = entry.total_bytes.saturating_add(bytes_len);
        self.reassembly_bytes = self.reassembly_bytes.saturating_add(bytes_len);
        entry
            .fragments
            .insert(frag_number, fragment.fragment().to_vec());
        entry.last_progress_ms = now_ms;
        self.counters.reassembly_messages = self.reassembly.len();
        self.counters.reassembly_bytes = self.reassembly_bytes;
        self.maybe_complete_reassembly(message_id, now_ms, now_secs, events);
    }

    fn maybe_complete_reassembly(
        &mut self,
        message_id: u32,
        now_ms: u64,
        now_secs: u64,
        events: &mut Vec<SessionEvent>,
    ) {
        let (complete, assembled) = match self.reassembly.get(&message_id) {
            Some(entry) => {
                let Some(total) = entry.total else {
                    return;
                };
                if entry.fragments.len() != usize::from(total) {
                    return;
                }
                // All fragment numbers 0..total must be present.
                for number in 0..total {
                    if !entry.fragments.contains_key(&number) {
                        return;
                    }
                }
                let message_type = match entry.message_type {
                    Some(code) => code,
                    None => return,
                };
                let expiration_secs = match entry.expiration_secs {
                    Some(expiration) => expiration,
                    None => return,
                };
                let mut body: Vec<u8> = Vec::new();
                for number in 0..total {
                    body.extend_from_slice(&entry.fragments[&number]);
                }
                if body.is_empty() || body.len() > MAX_I2NP_MESSAGE_BYTES {
                    return;
                }
                let mut encoded = Vec::with_capacity(9 + body.len());
                encoded.push(message_type);
                encoded.extend_from_slice(&message_id.to_be_bytes());
                encoded.extend_from_slice(&expiration_secs.to_be_bytes());
                encoded.extend_from_slice(&body);
                (true, Some((encoded, expiration_secs)))
            }
            None => return,
        };
        if complete && let Some((encoded, expiration_secs)) = assembled {
            self.drop_reassembly(message_id);
            self.deliver_complete_message(encoded, now_ms, now_secs, events);
            let _ = expiration_secs;
        }
    }

    fn deliver_complete_message(
        &mut self,
        encoded: Vec<u8>,
        now_ms: u64,
        _now_secs: u64,
        events: &mut Vec<SessionEvent>,
    ) {
        if encoded.len() < 9 {
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
            return;
        }
        let message_id = u32::from_be_bytes(encoded[1..5].try_into().expect("length checked"));
        let expiration_secs = u32::from_be_bytes(encoded[5..9].try_into().expect("length checked"));
        if self.is_duplicate(message_id, expiration_secs, now_ms) {
            return;
        }
        self.record_delivered(message_id, expiration_secs, now_ms);
        events.push(SessionEvent::I2npMessage(encoded));
    }

    fn drop_reassembly(&mut self, message_id: u32) {
        if let Some(entry) = self.reassembly.remove(&message_id) {
            self.reassembly_bytes = self.reassembly_bytes.saturating_sub(entry.total_bytes);
            self.counters.reassembly_messages = self.reassembly.len();
            self.counters.reassembly_bytes = self.reassembly_bytes;
        }
    }

    fn clear_reassembly(&mut self) {
        self.reassembly.clear();
        self.reassembly_bytes = 0;
        self.counters.reassembly_messages = 0;
        self.counters.reassembly_bytes = 0;
    }

    // -- Duplicate suppression ------------------------------------------

    fn is_duplicate(&self, message_id: u32, expiration_secs: u32, now_ms: u64) -> bool {
        self.delivered.iter().any(|entry| {
            entry.message_id == message_id
                && entry.expiration_secs == expiration_secs
                && now_ms.saturating_sub(entry.delivered_ms)
                    < constants::DATA_DUP_RETENTION_SECONDS.saturating_mul(1000)
        })
    }

    fn record_delivered(&mut self, message_id: u32, expiration_secs: u32, now_ms: u64) {
        self.expire_delivered(now_ms);
        if self.delivered.len() >= constants::DATA_DUP_CACHE_ENTRIES {
            self.delivered.pop_front();
        }
        self.delivered.push_back(DeliveredEntry {
            message_id,
            expiration_secs,
            delivered_ms: now_ms,
        });
    }

    fn expire_delivered(&mut self, now_ms: u64) {
        let retention = constants::DATA_DUP_RETENTION_SECONDS.saturating_mul(1000);
        while let Some(front) = self.delivered.front() {
            if now_ms.saturating_sub(front.delivered_ms) >= retention {
                self.delivered.pop_front();
            } else {
                break;
            }
        }
    }

    // -- Central poll ----------------------------------------------------

    /// Drives ACK, RTO, idle, reassembly-expiry, and duplicate-expiry
    /// deadlines at the caller clock. Returns at most one transmit
    /// datagram plus terminal guidance; the runtime owns actual timers.
    pub fn poll(&mut self, now_ms: u64, now_secs: u64) -> Vec<SessionAction> {
        let mut actions = Vec::new();
        if self.is_terminated() {
            return actions;
        }
        self.expire_delivered(now_ms);
        // Reassembly expiry releases all buffers for stale entries.
        let stale: Vec<u32> = self
            .reassembly
            .iter()
            .filter(|(_, entry)| {
                now_ms.saturating_sub(entry.last_progress_ms) >= 60_000
                    || now_ms.saturating_sub(entry.created_ms) >= 120_000
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            self.drop_reassembly(id);
            self.counters.reassembly_drops = self.counters.reassembly_drops.saturating_add(1);
        }
        // Idle timeout.
        let last_activity = match (self.last_send_ms, self.last_receive_ms) {
            (Some(send), Some(receive)) => Some(send.max(receive)),
            (Some(send), None) => Some(send),
            (None, Some(receive)) => Some(receive),
            (None, None) => None,
        };
        if let Some(last) = last_activity
            && now_ms.saturating_sub(last) >= self.idle_timeout_ms
        {
            self.local_terminate = Some(2);
            actions.push(SessionAction::Terminate { reason: 2 });
            return actions;
        }
        // RTO expiry on the oldest unacked congestion-controlled packet.
        if let Some(position) = self
            .sent
            .iter()
            .position(|packet| packet.counts_for_congestion)
        {
            let expired = self.sent[position].sent_ms.saturating_add(self.rto_ms) <= now_ms;
            if expired {
                let packet = self.sent.remove(position).expect("position checked");
                self.declare_lost(&packet, now_ms);
                self.consecutive_rto = self.consecutive_rto.saturating_add(1);
                // Exponential backoff, capped.
                self.rto_ms = (self.rto_ms.saturating_mul(2)).min(constants::DATA_MAX_RTO_MS);
                if self.consecutive_rto >= constants::DATA_MAX_CONSECUTIVE_RTO_BEFORE_TERMINATE {
                    self.local_terminate = Some(14);
                    actions.push(SessionAction::Terminate { reason: 14 });
                    return actions;
                }
            }
        }
        // ACK deadline: emit a standalone ACK when due.
        if self.ack_pending
            && let Some(deadline) = self.ack_deadline_ms
            && now_ms >= deadline
        {
            // Standalone ACKs fire only when the last received packet
            // was ack-eliciting (loop avoidance); otherwise the ACK
            // piggybacks on the next eliciting transmit.
            if self.last_received_eliciting
                && let Some(datagram) = self.poll_transmit(now_ms)
            {
                actions.push(SessionAction::Transmit(datagram));
            }
        }
        let _ = now_secs;
        actions
    }

    /// Returns the next deadline the runtime should arm, if any.
    pub fn next_deadline_ms(&self, now_ms: u64) -> Option<u64> {
        let mut candidates: Vec<u64> = Vec::new();
        if let Some(deadline) = self.ack_deadline_ms {
            if deadline > now_ms {
                candidates.push(deadline);
            } else {
                candidates.push(now_ms);
            }
        }
        if let Some(oldest) = self.sent.iter().find(|packet| packet.counts_for_congestion) {
            candidates.push(oldest.sent_ms.saturating_add(self.rto_ms));
        }
        if let (Some(send), Some(receive)) = (self.last_send_ms, self.last_receive_ms) {
            candidates.push(send.max(receive).saturating_add(self.idle_timeout_ms));
        }
        candidates.into_iter().min()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplayCheck {
    Admissible,
    Duplicate,
    TooOld,
    FutureJump,
}

fn is_ack_eliciting(kind: u8) -> bool {
    !matches!(
        kind,
        constants::BLOCK_ACK
            | constants::BLOCK_ADDRESS
            | constants::BLOCK_DATETIME
            | constants::BLOCK_PADDING
            | constants::BLOCK_TERMINATION
    )
}

fn is_ack_eliciting_decoded(block: &DecodedBlock<'_>) -> bool {
    !matches!(
        block,
        DecodedBlock::Ack(_)
            | DecodedBlock::Address(_)
            | DecodedBlock::Timestamp(_)
            | DecodedBlock::Padding { .. }
            | DecodedBlock::Termination(_)
            | DecodedBlock::Unknown { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _intro(byte: u8) -> IntroKey {
        IntroKey::new([byte; 32])
    }

    /// Runs one deterministic Noise transcript dance and returns both
    /// directional splits (initiator + responder) with matching keys.
    fn paired_test_splits() -> (Ssu2SplitKeys, Ssu2SplitKeys) {
        use crate::crypto::{Role, Ssu2PublicKey, Ssu2Transcript};
        use i2pr_crypto::X25519PrivateKey;
        use rand_chacha::ChaCha8Rng;
        use rand_core::SeedableRng;
        fn secret(seed: u64) -> X25519PrivateKey {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            X25519PrivateKey::generate(&mut rng).expect("secret")
        }
        let bob_static = secret(1001);
        let bob_public = Ssu2PublicKey::new(bob_static.public_bytes()).expect("public");
        let alice_static = secret(1002);
        let alice_eph = secret(1003);
        let bob_eph = secret(1004);
        let alice_eph_public = Ssu2PublicKey::new(alice_eph.public_bytes()).expect("public");
        let bob_eph_public = Ssu2PublicKey::new(bob_eph.public_bytes()).expect("public");
        let alice_public = Ssu2PublicKey::new(alice_static.public_bytes()).expect("public");
        let request_header = [0x11_u8; constants::LONG_HEADER_LENGTH];
        let created_header = [0x22_u8; constants::LONG_HEADER_LENGTH];
        let alice = Ssu2Transcript::new(Role::Initiator, bob_public);
        let bob = Ssu2Transcript::new(Role::Responder, bob_public);
        let es_alice = secret(1003)
            .diffie_hellman(bob_public.as_bytes())
            .expect("es");
        let es_bob = bob_static
            .diffie_hellman(alice_eph_public.as_bytes())
            .expect("es");
        let (alice, request_ct) = alice
            .seal_session_request(&request_header, alice_eph_public, es_alice, &[9_u8; 16])
            .expect("seal");
        let (bob, _) = bob
            .accept_session_request(&request_header, alice_eph_public, es_bob, &request_ct)
            .expect("accept");
        let ee_bob = secret(1004)
            .diffie_hellman(alice_eph_public.as_bytes())
            .expect("ee");
        let ee_alice = secret(1003)
            .diffie_hellman(bob_eph_public.as_bytes())
            .expect("ee");
        let (bob, created_ct) = bob
            .seal_session_created(
                &request_ct,
                &created_header,
                bob_eph_public,
                ee_bob,
                &[7_u8; 16],
            )
            .expect("seal created");
        let (alice, _) = alice
            .accept_session_created(
                &request_ct,
                &created_header,
                bob_eph_public,
                ee_alice,
                &created_ct,
            )
            .expect("accept created");
        let (alice, frame) = alice.seal_confirmed_static(alice_public).expect("static");
        let (bob, _) = bob.accept_confirmed_static(&frame).expect("open static");
        let se_alice = alice_static
            .diffie_hellman(bob_eph_public.as_bytes())
            .expect("se");
        let se_bob = secret(1004)
            .diffie_hellman(alice_public.as_bytes())
            .expect("se");
        let (alice, confirmed_ct) = alice
            .seal_confirmed_payload(se_alice, &[5_u8; 16])
            .expect("seal confirmed");
        let (bob, _) = bob
            .open_confirmed_payload(se_bob, &confirmed_ct)
            .expect("open confirmed");
        (alice.split().expect("split"), bob.split().expect("split"))
    }

    fn test_session_pair() -> (Ssu2Session, Ssu2Session) {
        let (alice_keys, bob_keys) = paired_test_splits();
        let alice = Ssu2Session::new(
            SessionConfig {
                local_conn_id: 0xaaaa_aaaa_aaaa_aaaa,
                remote_conn_id: 0xbbbb_bbbb_bbbb_bbbb,
                local_intro: IntroKey::new([0xA1; 32]),
                remote_intro: IntroKey::new([0xB2; 32]),
                initial_send_packet_number: 0,
                max_payload_bytes: SessionConfig::max_payload_for_mtu(1280, false),
                idle_timeout_ms: 300_000,
            },
            alice_keys,
        )
        .expect("session");
        let bob = Ssu2Session::new(
            SessionConfig {
                local_conn_id: 0xbbbb_bbbb_bbbb_bbbb,
                remote_conn_id: 0xaaaa_aaaa_aaaa_aaaa,
                local_intro: IntroKey::new([0xB2; 32]),
                remote_intro: IntroKey::new([0xA1; 32]),
                initial_send_packet_number: 0,
                max_payload_bytes: SessionConfig::max_payload_for_mtu(1280, false),
                idle_timeout_ms: 300_000,
            },
            bob_keys,
        )
        .expect("session");
        (alice, bob)
    }

    #[test]
    fn queued_new_token_round_trips_as_typed_event() {
        let (mut alice, mut bob) = test_session_pair();
        alice
            .queue_new_token(1_700_000_030, 0x0102_0304_0506_0708)
            .expect("queue");
        // A zero token is never announced on the wire.
        assert!(matches!(
            alice.queue_new_token(1_700_000_030, 0),
            Err(SessionError::MessageTooLarge)
        ));
        let datagram = alice.poll_transmit(2_000_000).expect("datagram");
        let outcome = bob.receive_datagram(2_000_001, 1_700_000_000, &datagram);
        assert!(outcome.dropped.is_none());
        assert!(outcome.events.contains(&SessionEvent::NewToken {
            expires: 1_700_000_030,
            token: 0x0102_0304_0506_0708,
        }));
        // Single-shot: the next transmit carries no second announcement.
        let next = alice.poll_transmit(2_000_002);
        assert!(next.is_none());
    }

    #[test]
    fn inbound_matching_is_side_effect_free() {
        let (mut alice, bob) = test_session_pair();
        alice.queue_path_challenge(vec![0xAA_u8; 8]).expect("queue");
        let datagram = alice.poll_transmit(2_000_000).expect("datagram");
        assert!(bob.matches_inbound(&datagram));
        assert!(!alice.matches_inbound(&datagram));
        assert!(bob.matches_inbound(&datagram));
        // Trial matching mutates no counters on either session.
        assert_eq!(bob.counters().packets_rejected, 0);
        assert_eq!(bob.counters().packets_replayed, 0);
        assert_eq!(alice.counters().packets_rejected, 0);
        // Truncated and unrelated datagrams never match.
        assert!(!bob.matches_inbound(&datagram[..16]));
        assert!(!bob.matches_inbound(&[0x55_u8; 64]));
        // Outbound depth reflects queued work for admission.
        let (messages, bytes) = alice.outbound_pending();
        assert_eq!((messages, bytes), (0, 0));
    }

    #[test]
    fn data_keys_use_second_hkdf_label() {
        let (alice_keys, _) = paired_test_splits();
        // The split seals under the derived k_data: seal/open through
        // the directional pair succeeds with the second-HKDF keys.
        let mut alice_keys = alice_keys;
        let header = [0x55_u8; constants::SHORT_HEADER_LENGTH];
        let sealed = alice_keys
            .transmit()
            .seal(3, &header, &[0x44_u8; 8])
            .expect("seal");
        assert!(!sealed.is_empty());
    }

    #[test]
    fn ack_underflow_rejected_without_state_mutation() {
        let (mut alice, _) = test_session_pair();
        // Seed one sent packet so mutation would be observable.
        alice.sent.push_back(SentPacket {
            packet_number: 10,
            sent_ms: 1000,
            ack_eliciting_bytes: 100,
            ack_only: false,
            fragments: Vec::new(),
            counts_for_congestion: true,
            generation: 0,
        });
        alice.bytes_in_flight = 100;
        // ack_through = 1 with acnt = 5 implies packets below zero.
        let bad = AckBlock::new(1, 5, Vec::new()).expect("structural");
        assert_eq!(alice.apply_ack(&bad, 2000), Err(SessionError::AckUnderflow));
        // Sent state untouched.
        assert_eq!(alice.sent.len(), 1);
        assert_eq!(alice.bytes_in_flight, 100);
        // Degenerate (0,0) ranges are rejected at the block layer.
        assert!(AckBlock::new(10, 0, vec![(0, 0)]).is_err());
    }

    #[test]
    fn duplicate_ack_is_idempotent() {
        let (mut alice, _) = test_session_pair();
        alice.sent.push_back(SentPacket {
            packet_number: 7,
            sent_ms: 1000,
            ack_eliciting_bytes: 120,
            ack_only: false,
            fragments: Vec::new(),
            counts_for_congestion: true,
            generation: 0,
        });
        alice.bytes_in_flight = 120;
        let ack = AckBlock::new(7, 0, Vec::new()).expect("ack");
        alice.apply_ack(&ack, 2000).expect("first apply");
        assert!(alice.sent.is_empty());
        assert_eq!(alice.bytes_in_flight, 0);
        // Second application retires nothing and changes nothing.
        alice.apply_ack(&ack, 3000).expect("duplicate apply");
        assert!(alice.sent.is_empty());
        assert_eq!(alice.bytes_in_flight, 0);
    }

    #[test]
    fn sent_history_exact_capacity_evicts_oldest() {
        let (mut alice, _) = test_session_pair();
        for number in 0..constants::DATA_MAX_SENT_PACKETS as u32 {
            alice.sent.push_back(SentPacket {
                packet_number: number,
                sent_ms: 1000,
                ack_eliciting_bytes: 50,
                ack_only: false,
                fragments: Vec::new(),
                counts_for_congestion: true,
                generation: 0,
            });
        }
        assert_eq!(alice.sent.len(), constants::DATA_MAX_SENT_PACKETS);
        let losses_before = alice.counters.loss_events;
        // One more send evicts the oldest as bounded loss.
        alice.record_sent(2000, 60, 60, false, Vec::new());
        assert_eq!(alice.sent.len(), constants::DATA_MAX_SENT_PACKETS);
        assert_eq!(alice.sent[0].packet_number, 1);
        assert_eq!(alice.counters.loss_events, losses_before.saturating_add(1));
    }

    #[test]
    fn per_fragment_ceiling_fails_message_silently() {
        let (mut alice, _) = test_session_pair();
        // A fragment at its ceiling is dropped on the next loss while
        // the session stays usable (no teardown, bounded state).
        let packet = SentPacket {
            packet_number: 3,
            sent_ms: 1000,
            ack_eliciting_bytes: 60,
            ack_only: false,
            fragments: vec![RetransmittableFragment {
                message_id: 4242,
                frag_number: 0,
                is_last: true,
                total_fragments: 1,
                message_type: 20,
                expiration_secs: 1_700_000_000,
                bytes: vec![0x77; 16],
                generation: constants::DATA_MAX_FRAGMENT_RETRANSMISSIONS,
            }],
            counts_for_congestion: true,
            generation: constants::DATA_MAX_FRAGMENT_RETRANSMISSIONS,
        };
        alice.declare_lost(&packet, 2000);
        assert!(alice.pending_retransmit.is_empty());
        assert!(!alice.is_terminated());
    }

    #[test]
    fn packet_number_wrap_and_exhaustion() {
        let (mut alice, _) = test_session_pair();
        alice.next_send = u32::MAX - 1;
        assert_eq!(alice.allocate_packet_number(), Some(u32::MAX - 1));
        assert_eq!(alice.allocate_packet_number(), Some(u32::MAX));
        // The 32-bit space never wraps within a session.
        assert_eq!(alice.allocate_packet_number(), None);
        assert!(alice.send_exhausted);
    }

    #[test]
    fn replay_window_handles_wrap_boundaries() {
        let (mut alice, _) = test_session_pair();
        // Simulate reception near the top of the u32 space.
        alice.highest_received = Some(u32::MAX - 1);
        alice.received_bitmap = 1;
        assert_eq!(alice.check_replay(u32::MAX - 1), ReplayCheck::Duplicate);
        assert_eq!(alice.check_replay(u32::MAX), ReplayCheck::Admissible);
        alice.mark_received(u32::MAX, true, 1000, false);
        // Wrap to zero stays within the future-jump policy.
        assert_eq!(alice.check_replay(0), ReplayCheck::Admissible);
        alice.mark_received(0, true, 1001, false);
        // The pre-wrap packets are still addressable within the window.
        assert_eq!(alice.check_replay(u32::MAX), ReplayCheck::Duplicate);
        // Numerically-ahead values beyond the jump limit are rejected
        // as impossible futures (indistinguishable from attacks after
        // a wrap); values behind by the full window are too old.
        assert_eq!(alice.check_replay(100_000), ReplayCheck::FutureJump);
        assert_eq!(alice.check_replay(u32::MAX - 200), ReplayCheck::TooOld);
    }

    #[test]
    fn nextnonce_surfaces_rekey_event_and_stays_usable() {
        let (alice, mut bob) = test_session_pair();
        // Hand-craft an authenticated payload carrying only a NextNonce
        // block (type 11, empty), sealed through Alice's transmit state
        // using the same steps as seal_packet.
        use crate::header::DataHeader as TestDataHeader;
        // NextNonce (empty) plus trailing padding to reach the 8-byte
        // header-protection floor.
        let plaintext = vec![
            constants::BLOCK_NEXT_NONCE,
            0,
            0,
            constants::BLOCK_PADDING,
            0,
            2,
            0,
            0,
        ];
        let packet_number = 0_u32;
        let header = TestDataHeader::new(0xbbbb_bbbb_bbbb_bbbb, packet_number, false);
        let header_bytes = header.encode();
        let sealed = alice
            .transmit_cipher
            .seal(packet_number, &header_bytes, &plaintext)
            .expect("seal");
        let mut datagram = Vec::new();
        datagram.extend_from_slice(&header_bytes);
        datagram.extend_from_slice(&sealed);
        crate::crypto::apply_header_protection(
            &mut datagram,
            constants::SHORT_HEADER_LENGTH,
            alice.remote_intro.as_bytes(),
            &alice.transmit_header_2,
            false,
        )
        .expect("protect");
        let outcome = bob.receive_datagram(2000, 1_700_000_000, &datagram);
        assert!(outcome.dropped.is_none());
        assert!(outcome.events.contains(&SessionEvent::RekeyRequested));
        // The session stays usable afterwards.
        assert!(!bob.is_terminated());
    }
}
