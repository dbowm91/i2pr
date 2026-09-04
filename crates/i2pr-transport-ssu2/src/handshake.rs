//! SSU2 v2 establishment message codecs and RouterInfo binding.
//!
//! This module owns the strict, bounded TokenRequest/Retry/
//! SessionRequest/SessionCreated/SessionConfirmed datagram codecs, the
//! cheap symmetric-only prevalidation layer, the SessionConfirmed
//! fragment reassembly, and the deep RouterInfo establishment check
//! that binds the handshake static key before an authenticated peer is
//! exposed. No NetDB state is mutated here; validated material is
//! returned to the caller.
//!
//! Normative traceability: SSU2 specification sections Session
//! Establishment, Packet Header, Header Encryption KDF, Packet
//! Integrity, KDF for Session Request, KDF for Session Created and
//! Session Confirmed part 1, KDF for Session Confirmed part 2, KDF for
//! Retry, KDF for Token Request, SessionRequest/ SessionCreated/
//! SessionConfirmed/Retry/TokenRequest message sections, and Session
//! Confirmed Fragmentation.

use std::{fmt, vec::Vec};

use i2pr_crypto::{router_identity_hash, verify_router_info};
use i2pr_proto::{CryptoKeyType, Hash, RouterInfo};
use thiserror::Error;

use crate::address::decode_i2p_base64;
use crate::block::{
    AddressBlock, Block, BlockError, DecodedBlock, PaddingBlock, ParsedBlocks, RouterInfoBlock,
    TerminationBlock, TimestampBlock, encode_blocks, parse_blocks,
};
use crate::constants;
use crate::crypto::{IntroKey, Ssu2CryptoError, Ssu2PublicKey};
use crate::header::{HeaderError, LongHeader, MessageType, SessionConfirmedHeader};
use crate::packet::{DatagramLengthClass, PacketError};

/// Typed failures from SSU2 establishment message processing.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum HandshakeError {
    /// Fewer bytes than the message form requires.
    #[error("truncated SSU2 handshake message")]
    Truncated,
    /// A datagram or payload exceeded its bounded maximum.
    #[error("SSU2 handshake message exceeds its bound")]
    TooLong,
    /// A payload was smaller than the handshake minimum.
    #[error("SSU2 handshake payload is below the minimum")]
    PayloadTooShort,
    /// A header field was structurally invalid.
    #[error("SSU2 handshake header is invalid")]
    Header(#[from] HeaderError),
    /// A datagram length class was invalid.
    #[error("SSU2 handshake datagram length is invalid")]
    Packet(#[from] PacketError),
    /// A block sequence was malformed or violated ordering rules.
    #[error("SSU2 handshake block sequence is invalid")]
    Blocks(#[from] BlockError),
    /// A cryptographic transcript or protection operation failed.
    #[error("SSU2 handshake crypto operation failed")]
    Crypto(#[from] Ssu2CryptoError),
    /// A required DateTime block was missing or malformed.
    #[error("SSU2 handshake payload has no valid timestamp")]
    MissingTimestamp,
    /// A peer timestamp fell outside the skew policy.
    #[error("SSU2 handshake timestamp is stale")]
    StaleTimestamp,
    /// A peer timestamp fell outside the skew policy.
    #[error("SSU2 handshake timestamp is in the future")]
    FutureTimestamp,
    /// An establishment value was already observed in the replay window.
    #[error("SSU2 handshake replay detected")]
    ReplayDetected,
    /// The bounded replay cache has no admission capacity.
    #[error("SSU2 handshake replay cache is full")]
    ReplayCacheFull,
    /// A Retry token failed validation (absent, unknown, expired,
    /// reused, or bound to a different source).
    #[error("SSU2 address-validation token rejected")]
    TokenRejected,
    /// A Retry response would exceed the amplification budget.
    #[error("SSU2 Retry response exceeds the amplification budget")]
    AmplificationExceeded,
    /// A SessionConfirmed fragment was malformed or inconsistent.
    #[error("SSU2 SessionConfirmed fragment is invalid")]
    BadFragment,
    /// A SessionConfirmed fragment was duplicated or conflicting.
    #[error("SSU2 SessionConfirmed fragment is duplicated")]
    DuplicateFragment,
    /// SessionConfirmed reassembly is missing fragments.
    #[error("SSU2 SessionConfirmed reassembly is incomplete")]
    IncompleteFragments,
    /// Reassembled establishment bytes exceeded the aggregate ceiling.
    #[error("SSU2 reassembled establishment bytes exceed the ceiling")]
    AggregateTooLarge,
    /// The RouterInfo bytes were not a bounded complete structure.
    #[error("malformed RouterInfo in SSU2 handshake")]
    RouterInfoMalformed,
    /// The RouterInfo signature was not valid for its signed region.
    #[error("RouterInfo signature invalid")]
    RouterInfoSignatureInvalid,
    /// The RouterInfo hash did not match the dial/accept expectation.
    #[error("SSU2 peer identity mismatch")]
    PeerIdentityMismatch,
    /// The RouterInfo uses a key or signature type outside this implementation.
    #[error("unsupported SSU2 peer key or signature type")]
    UnsupportedPeerKey,
    /// No SSU2 address in the RouterInfo matched the required version.
    #[error("SSU2 RouterInfo carries no compatible SSU2 address")]
    MissingSsu2Address,
    /// The SSU2 static key in RouterInfo did not match the handshake peer.
    #[error("SSU2 transport static-key mismatch")]
    TransportStaticKeyMismatch,
    /// The SSU2 intro key shape was invalid where required.
    #[error("SSU2 intro key is invalid")]
    IntroKeyInvalid,
    /// The RouterInfo publication time fell outside the freshness policy.
    #[error("SSU2 RouterInfo publication time is stale")]
    RouterInfoStale,
    /// The RouterInfo publication time fell outside the freshness policy.
    #[error("SSU2 RouterInfo publication time is in the future")]
    RouterInfoFuture,
    /// The RouterInfo was not the first SessionConfirmed block.
    #[error("SSU2 RouterInfo is not the first establishment block")]
    RouterInfoNotFirst,
    /// A local establishment policy denied the operation.
    #[error("SSU2 local establishment policy denied the operation")]
    LocalPolicyDenied,
}

/// Bounded clock-skew policy for handshake DateTime blocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockSkewPolicy {
    maximum_delta: u64,
}

impl ClockSkewPolicy {
    /// Creates the policy with an explicit skew bound in seconds.
    pub const fn new(maximum_delta: u64) -> Self {
        Self { maximum_delta }
    }

    /// Returns the specification handshake policy (+/- 2 minutes).
    pub const fn handshake() -> Self {
        Self {
            maximum_delta: constants::HANDSHAKE_MAX_CLOCK_SKEW_SECONDS,
        }
    }

    /// Returns the maximum accepted skew in seconds.
    pub const fn maximum_delta(self) -> u64 {
        self.maximum_delta
    }

    /// Minimum replay retention in seconds (at least 2*D per spec).
    pub const fn replay_retention(self) -> u64 {
        self.maximum_delta.saturating_mul(2)
    }

    /// Classifies a peer timestamp (Unix seconds) against local time.
    pub fn classify(self, local: u64, peer: u32) -> Result<(), HandshakeError> {
        let delta = i128::from(peer) - i128::from(local);
        if delta < -i128::from(self.maximum_delta) {
            Err(HandshakeError::StaleTimestamp)
        } else if delta > i128::from(self.maximum_delta) {
            Err(HandshakeError::FutureTimestamp)
        } else {
            Ok(())
        }
    }
}

/// A fixed-size replay token with redacted diagnostics.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ReplayToken([u8; 32]);

impl ReplayToken {
    /// Derives a token from the (possibly obfuscated) ephemeral field.
    pub fn from_ephemeral_bytes(bytes: &[u8]) -> Self {
        Self(*i2pr_crypto::sha256(bytes).as_bytes())
    }

    /// Borrows the exact cache key bytes for an injected cache.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ReplayToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReplayToken(<redacted>)")
    }
}

/// A replay-cache admission result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayDecision {
    /// The token was not present and has been reserved.
    Fresh,
    /// The token was already observed within the retention window.
    Replayed,
    /// The bounded cache has no admission capacity.
    CacheFull,
}

/// A deterministic bounded replay cache for establishment ephemerals.
pub struct HandshakeReplayCache {
    retention: u64,
    maximum_entries: usize,
    entries: Vec<(ReplayToken, u64)>,
}

impl HandshakeReplayCache {
    /// Creates an empty cache with explicit capacity and retention.
    pub fn new(maximum_entries: usize, retention: u64) -> Result<Self, HandshakeError> {
        if maximum_entries == 0
            || retention == 0
            || maximum_entries > constants::MAX_HANDSHAKE_REPLAY_ENTRIES
        {
            return Err(HandshakeError::LocalPolicyDenied);
        }
        Ok(Self {
            retention,
            maximum_entries,
            entries: Vec::new(),
        })
    }

    /// Checks and reserves a token, expiring entries deterministically first.
    pub fn check_and_record(&mut self, token: ReplayToken, now: u64) -> ReplayDecision {
        self.entries
            .retain(|(_, seen)| now.saturating_sub(*seen) < self.retention);
        if self.entries.iter().any(|(known, _)| *known == token) {
            return ReplayDecision::Replayed;
        }
        if self.entries.len() >= self.maximum_entries {
            return ReplayDecision::CacheFull;
        }
        self.entries.push((token, now));
        ReplayDecision::Fresh
    }

    /// Returns the current bounded entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the cache has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A cheaply classified inbound long-header datagram: length, header
/// protection, and structural header checks passed using symmetric
/// operations only. No DH, no payload AEAD, and no session allocation
/// occurred on this path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrevalidatedLongDatagram {
    header: LongHeader,
    rest_length: usize,
}

impl PrevalidatedLongDatagram {
    /// Returns the structurally validated long header.
    pub const fn header(self) -> LongHeader {
        self.header
    }

    /// Returns the post-header byte length (ephemeral key plus
    /// authenticated tail, or authenticated tail only).
    pub const fn rest_length(self) -> usize {
        self.rest_length
    }
}

/// Cheap prevalidation for inbound TokenRequest/SessionRequest/Retry
/// datagrams: datagram length class, header deprotection with the local
/// intro key, exact header decode, version/network/type checks, and the
/// minimum-tail bound. All operations are symmetric and allocation-free
/// beyond the caller's deprotection buffer.
pub fn prevalidate_long_datagram(
    datagram: &mut [u8],
    intro_key: &IntroKey,
    expected_type: MessageType,
    protect_ephemeral_tail: bool,
) -> Result<PrevalidatedLongDatagram, HandshakeError> {
    DatagramLengthClass::classify(datagram.len())?;
    if datagram.len() < constants::LONG_HEADER_LENGTH {
        return Err(HandshakeError::Truncated);
    }
    crate::crypto::remove_header_protection(
        datagram,
        constants::LONG_HEADER_LENGTH,
        intro_key.as_bytes(),
        intro_key.as_bytes(),
        protect_ephemeral_tail,
    )
    .map_err(|_| HandshakeError::Truncated)?;
    let header = LongHeader::decode(&datagram[..constants::LONG_HEADER_LENGTH])?;
    if header.message_type() != expected_type {
        return Err(HandshakeError::Truncated);
    }
    let rest_length = datagram.len() - constants::LONG_HEADER_LENGTH;
    let minimum_rest = match expected_type {
        MessageType::SessionRequest => {
            constants::HANDSHAKE_EPHEMERAL_LENGTH + constants::MIN_POST_HEADER_BYTES
        }
        _ => constants::MIN_POST_HEADER_BYTES,
    };
    if rest_length < minimum_rest {
        return Err(HandshakeError::PayloadTooShort);
    }
    Ok(PrevalidatedLongDatagram {
        header,
        rest_length,
    })
}

/// A parsed TokenRequest: header plus validated payload timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenRequest {
    header: LongHeader,
    timestamp: u32,
}

impl TokenRequest {
    /// Returns the validated long header.
    pub const fn header(self) -> LongHeader {
        self.header
    }

    /// Returns the payload timestamp (Unix seconds).
    pub const fn timestamp(self) -> u32 {
        self.timestamp
    }
}

/// Builds an outbound TokenRequest datagram: long header, intro-key
/// AEAD payload (DateTime plus caller padding, minimum 8 bytes), and
/// header protection under the responder intro key.
pub fn build_token_request(
    intro_key: &IntroKey,
    src_conn_id: u64,
    dst_conn_id: u64,
    packet_number: u32,
    timestamp: u32,
    padding: Vec<u8>,
) -> Result<Vec<u8>, HandshakeError> {
    let mut blocks = Vec::with_capacity(2);
    blocks.push(Block::Timestamp(TimestampBlock::new(timestamp)));
    // The DateTime block alone is 7 bytes; the handshake minimum is 8,
    // so empty caller padding tops up with one zero byte.
    let padding = if padding.is_empty() {
        vec![0_u8; 1]
    } else {
        padding
    };
    blocks.push(Block::Padding(PaddingBlock::new(padding)?));
    let payload = encode_blocks(blocks)?;
    if payload.len() < constants::MIN_HANDSHAKE_PAYLOAD_BYTES {
        return Err(HandshakeError::PayloadTooShort);
    }
    if payload.len() > constants::MAX_HANDSHAKE_PAYLOAD_BYTES {
        return Err(HandshakeError::TooLong);
    }
    let header = LongHeader::new(
        dst_conn_id,
        packet_number,
        MessageType::TokenRequest,
        src_conn_id,
        0,
    )
    .map_err(HandshakeError::Header)?;
    let header_bytes = header.encode();
    let sealed =
        crate::crypto::seal_token_payload(intro_key, packet_number, &header_bytes, &payload)?;
    let mut datagram = Vec::with_capacity(constants::LONG_HEADER_LENGTH + sealed.len());
    datagram.extend_from_slice(&header_bytes);
    datagram.extend_from_slice(&sealed);
    crate::crypto::apply_header_protection(
        &mut datagram,
        constants::LONG_HEADER_LENGTH,
        intro_key.as_bytes(),
        intro_key.as_bytes(),
        false,
    )?;
    Ok(datagram)
}

/// Parses an inbound TokenRequest after cheap prevalidation: opens the
/// intro-key payload, requires a DateTime block, and classifies its
/// skew. Expensive DH is never involved on this path.
pub fn parse_token_request(
    datagram: &mut [u8],
    intro_key: &IntroKey,
    skew: ClockSkewPolicy,
    now: u64,
) -> Result<TokenRequest, HandshakeError> {
    let pre = prevalidate_long_datagram(datagram, intro_key, MessageType::TokenRequest, false)?;
    let header = pre.header();
    let sealed = &datagram[constants::LONG_HEADER_LENGTH..];
    let payload = crate::crypto::open_token_payload(
        intro_key,
        header.packet_number(),
        &header.encode(),
        sealed,
    )?;
    let timestamp = require_timestamp(&payload)?;
    skew.classify(now, timestamp)?;
    Ok(TokenRequest { header, timestamp })
}

/// A parsed Retry: header, token, timestamp, and optional termination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryMessage {
    header: LongHeader,
    timestamp: u32,
    termination: Option<TerminationBlock>,
}

impl RetryMessage {
    /// Returns the validated long header (its token field carries the
    /// address-validation token, or zero with a termination block).
    pub const fn header(self) -> LongHeader {
        self.header
    }

    /// Returns the address-validation token from the header.
    pub const fn token(self) -> u64 {
        self.header.token()
    }

    /// Returns the payload timestamp (Unix seconds).
    pub const fn timestamp(self) -> u32 {
        self.timestamp
    }

    /// Returns the termination block when the session was rejected.
    pub const fn termination(self) -> Option<TerminationBlock> {
        self.termination
    }
}

/// Builds an outbound Retry datagram under the amplification budget:
/// the response must not exceed three times the request length.
///
/// The argument list is intentionally explicit: every Retry input is a
/// distinct protocol capability and bundling them would hide required
/// versus optional material.
#[allow(clippy::too_many_arguments)]
pub fn build_retry(
    intro_key: &IntroKey,
    request_length: usize,
    dst_conn_id: u64,
    src_conn_id: u64,
    packet_number: u32,
    token: u64,
    timestamp: u32,
    address: AddressBlock,
    termination: Option<TerminationBlock>,
    padding: Vec<u8>,
) -> Result<Vec<u8>, HandshakeError> {
    if token == 0 && termination.is_none() {
        return Err(HandshakeError::TokenRejected);
    }
    let mut blocks = Vec::with_capacity(5);
    blocks.push(Block::Timestamp(TimestampBlock::new(timestamp)));
    blocks.push(Block::Address(address));
    if let Some(termination) = termination {
        blocks.push(Block::Termination(termination));
    }
    if !padding.is_empty() {
        if padding.len() > constants::MAX_RETRY_PADDING_BYTES {
            return Err(HandshakeError::AmplificationExceeded);
        }
        blocks.push(Block::Padding(PaddingBlock::new(padding)?));
    }
    let payload = encode_blocks(blocks)?;
    let header = LongHeader::new(
        dst_conn_id,
        packet_number,
        MessageType::Retry,
        src_conn_id,
        token,
    )
    .map_err(HandshakeError::Header)?;
    let header_bytes = header.encode();
    let sealed =
        crate::crypto::seal_token_payload(intro_key, packet_number, &header_bytes, &payload)?;
    let mut datagram = Vec::with_capacity(constants::LONG_HEADER_LENGTH + sealed.len());
    datagram.extend_from_slice(&header_bytes);
    datagram.extend_from_slice(&sealed);
    crate::crypto::apply_header_protection(
        &mut datagram,
        constants::LONG_HEADER_LENGTH,
        intro_key.as_bytes(),
        intro_key.as_bytes(),
        false,
    )?;
    let budget = request_length
        .saturating_mul(constants::RETRY_AMPLIFICATION_NUMERATOR)
        .saturating_div(constants::RETRY_AMPLIFICATION_DENOMINATOR.max(1));
    if datagram.len() > budget {
        return Err(HandshakeError::AmplificationExceeded);
    }
    Ok(datagram)
}

/// Parses an inbound Retry: opens the intro-key payload and requires
/// DateTime plus Address blocks.
pub fn parse_retry(
    datagram: &mut [u8],
    intro_key: &IntroKey,
    skew: ClockSkewPolicy,
    now: u64,
) -> Result<RetryMessage, HandshakeError> {
    let pre = prevalidate_long_datagram(datagram, intro_key, MessageType::Retry, false)?;
    let header = pre.header();
    let sealed = &datagram[constants::LONG_HEADER_LENGTH..];
    let payload = crate::crypto::open_token_payload(
        intro_key,
        header.packet_number(),
        &header.encode(),
        sealed,
    )?;
    let parsed = parse_blocks(&payload)?;
    let mut timestamp = None;
    let mut seen_address = false;
    let mut termination = None;
    for block in parsed.blocks() {
        match block {
            DecodedBlock::Timestamp(value) => {
                if timestamp.is_some() {
                    return Err(HandshakeError::Blocks(BlockError::DuplicateBlock));
                }
                timestamp = Some(value.seconds());
            }
            DecodedBlock::Address(_) => {
                seen_address = true;
            }
            DecodedBlock::Termination(value) => {
                termination = Some(*value);
            }
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or(HandshakeError::MissingTimestamp)?;
    if !seen_address {
        return Err(HandshakeError::MissingTimestamp);
    }
    skew.classify(now, timestamp)?;
    if header.token() == 0 && termination.is_none() {
        return Err(HandshakeError::TokenRejected);
    }
    Ok(RetryMessage {
        header,
        timestamp,
        termination,
    })
}

/// A parsed SessionRequest: header, ephemeral key, and payload ciphertext.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRequestParts {
    /// The deprotected long header.
    pub header: LongHeader,
    /// The deobfuscated initiator ephemeral key.
    pub ephemeral: Ssu2PublicKey,
    /// The Noise payload ciphertext (without MAC-stripping).
    pub ciphertext: Vec<u8>,
}

/// Builds an outbound SessionRequest datagram: long header, ephemeral
/// key, transcript ciphertext, and header protection under the
/// responder intro key for the masks plus the derived next key where
/// the transcript stage requires it.
pub fn build_session_request(
    header: &LongHeader,
    ephemeral: &Ssu2PublicKey,
    payload_ciphertext: &[u8],
    k_header_1: &[u8; constants::KEY_LENGTH],
    k_header_2: &[u8; constants::KEY_LENGTH],
) -> Result<Vec<u8>, HandshakeError> {
    if header.message_type() != MessageType::SessionRequest {
        return Err(HandshakeError::Truncated);
    }
    let mut datagram = Vec::with_capacity(
        constants::LONG_HEADER_LENGTH + constants::KEY_LENGTH + payload_ciphertext.len(),
    );
    datagram.extend_from_slice(&header.encode());
    datagram.extend_from_slice(ephemeral.as_bytes());
    datagram.extend_from_slice(payload_ciphertext);
    crate::crypto::apply_header_protection(
        &mut datagram,
        constants::LONG_HEADER_LENGTH,
        k_header_1,
        k_header_2,
        true,
    )?;
    Ok(datagram)
}

/// Parses an inbound SessionRequest after cheap prevalidation: removes
/// header protection and splits header, ephemeral key, and payload
/// ciphertext. Payload AEAD/DH happens in the transcript, not here.
pub fn parse_session_request(
    datagram: &mut [u8],
    intro_key: &IntroKey,
) -> Result<SessionRequestParts, HandshakeError> {
    let pre = prevalidate_long_datagram(datagram, intro_key, MessageType::SessionRequest, true)?;
    let header = pre.header();
    let rest = &datagram[constants::LONG_HEADER_LENGTH..];
    let ephemeral_bytes: [u8; constants::KEY_LENGTH] = rest[..constants::KEY_LENGTH]
        .try_into()
        .map_err(|_| HandshakeError::Truncated)?;
    let ephemeral = Ssu2PublicKey::new(ephemeral_bytes)?;
    let ciphertext = rest[constants::KEY_LENGTH..].to_vec();
    Ok(SessionRequestParts {
        header,
        ephemeral,
        ciphertext,
    })
}

/// A parsed SessionCreated: header, ephemeral key, and payload ciphertext.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionCreatedParts {
    /// The deprotected long header.
    pub header: LongHeader,
    /// The deobfuscated responder ephemeral key.
    pub ephemeral: Ssu2PublicKey,
    /// The Noise payload ciphertext (without MAC-stripping).
    pub ciphertext: Vec<u8>,
}

/// Builds an outbound SessionCreated datagram with the derived
/// `SessCreateHeader` second protection key.
pub fn build_session_created(
    header: &LongHeader,
    ephemeral: &Ssu2PublicKey,
    payload_ciphertext: &[u8],
    k_header_1: &[u8; constants::KEY_LENGTH],
    k_header_2: &[u8; constants::KEY_LENGTH],
) -> Result<Vec<u8>, HandshakeError> {
    if header.message_type() != MessageType::SessionCreated {
        return Err(HandshakeError::Truncated);
    }
    let mut datagram = Vec::with_capacity(
        constants::LONG_HEADER_LENGTH + constants::KEY_LENGTH + payload_ciphertext.len(),
    );
    datagram.extend_from_slice(&header.encode());
    datagram.extend_from_slice(ephemeral.as_bytes());
    datagram.extend_from_slice(payload_ciphertext);
    crate::crypto::apply_header_protection(
        &mut datagram,
        constants::LONG_HEADER_LENGTH,
        k_header_1,
        k_header_2,
        true,
    )?;
    Ok(datagram)
}

/// Parses an inbound SessionCreated with the derived second protection key.
pub fn parse_session_created(
    datagram: &mut [u8],
    k_header_1: &[u8; constants::KEY_LENGTH],
    k_header_2: &[u8; constants::KEY_LENGTH],
) -> Result<SessionCreatedParts, HandshakeError> {
    DatagramLengthClass::classify(datagram.len())?;
    if datagram.len()
        < constants::LONG_HEADER_LENGTH
            + constants::HANDSHAKE_EPHEMERAL_LENGTH
            + constants::MIN_POST_HEADER_BYTES
    {
        return Err(HandshakeError::Truncated);
    }
    crate::crypto::remove_header_protection(
        datagram,
        constants::LONG_HEADER_LENGTH,
        k_header_1,
        k_header_2,
        true,
    )
    .map_err(|_| HandshakeError::Truncated)?;
    let header = LongHeader::decode(&datagram[..constants::LONG_HEADER_LENGTH])?;
    if header.message_type() != MessageType::SessionCreated {
        return Err(HandshakeError::Truncated);
    }
    let rest = &datagram[constants::LONG_HEADER_LENGTH..];
    let ephemeral_bytes: [u8; constants::KEY_LENGTH] = rest[..constants::KEY_LENGTH]
        .try_into()
        .map_err(|_| HandshakeError::Truncated)?;
    let ephemeral = Ssu2PublicKey::new(ephemeral_bytes)?;
    let ciphertext = rest[constants::KEY_LENGTH..].to_vec();
    Ok(SessionCreatedParts {
        header,
        ephemeral,
        ciphertext,
    })
}

/// Computes the SessionConfirmed fragment count for one jumbo
/// ciphertext under one per-fragment payload budget, enforcing the
/// same bounds `build_session_confirmed` enforces.
fn confirmed_fragment_total(jumbo_len: usize, mtu_payload: usize) -> Result<u8, HandshakeError> {
    if jumbo_len == 0 || jumbo_len > constants::MAX_CONFIRMED_REASSEMBLED_BYTES {
        return Err(HandshakeError::AggregateTooLarge);
    }
    if !(constants::MIN_POST_HEADER_BYTES..=constants::MAX_HANDSHAKE_PAYLOAD_BYTES)
        .contains(&mtu_payload)
    {
        return Err(HandshakeError::LocalPolicyDenied);
    }
    let total = jumbo_len.div_ceil(mtu_payload);
    if !(1..=constants::MAX_SESSION_CONFIRMED_FRAGMENTS).contains(&total) {
        return Err(HandshakeError::AggregateTooLarge);
    }
    #[allow(clippy::cast_possible_truncation)]
    let total_byte = total as u8;
    let last_length = jumbo_len - (total - 1) * mtu_payload;
    if last_length < constants::MIN_POST_HEADER_BYTES {
        return Err(HandshakeError::PayloadTooShort);
    }
    Ok(total_byte)
}

/// Encodes the first-fragment short header for one SessionConfirmed
/// jumbo ciphertext. The initiator mixes these exact bytes into the
/// Noise transcript before sealing the static-key frame, so the header
/// must be derived before the frame exists; the fragment builder below
/// reproduces the identical header from the same inputs.
pub fn session_confirmed_first_header(
    dst_conn_id: u64,
    jumbo_len: usize,
    mtu_payload: usize,
) -> Result<[u8; constants::SHORT_HEADER_LENGTH], HandshakeError> {
    let total = confirmed_fragment_total(jumbo_len, mtu_payload)?;
    let header =
        SessionConfirmedHeader::new(dst_conn_id, 0, total).map_err(HandshakeError::Header)?;
    Ok(header.encode())
}

/// Builds outbound SessionConfirmed fragment datagrams: short headers
/// carrying fragment `number/total` plus the jumbo ciphertext slices,
/// each protected with that datagram's own trailing MAC bytes. The
/// last fragment must carry at least 24 bytes so header protection has
/// its IV window.
pub fn build_session_confirmed(
    dst_conn_id: u64,
    jumbo_ciphertext: &[u8],
    mtu_payload: usize,
    k_header_1: &[u8; constants::KEY_LENGTH],
    k_header_2: &[u8; constants::KEY_LENGTH],
) -> Result<Vec<Vec<u8>>, HandshakeError> {
    let total_byte = confirmed_fragment_total(jumbo_ciphertext.len(), mtu_payload)?;
    let total = usize::from(total_byte);
    let mut datagrams = Vec::with_capacity(total);
    for (index, chunk) in jumbo_ciphertext.chunks(mtu_payload).enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let number = index as u8;
        let header = SessionConfirmedHeader::new(dst_conn_id, number, total_byte)
            .map_err(HandshakeError::Header)?;
        let mut datagram = Vec::with_capacity(constants::SHORT_HEADER_LENGTH + chunk.len());
        datagram.extend_from_slice(&header.encode());
        datagram.extend_from_slice(chunk);
        crate::crypto::apply_header_protection(
            &mut datagram,
            constants::SHORT_HEADER_LENGTH,
            k_header_1,
            k_header_2,
            false,
        )?;
        datagrams.push(datagram);
    }
    Ok(datagrams)
}

/// Bounded SessionConfirmed reassembly: collects fragments by number,
/// rejects duplicates/conflicts and aggregate overrun, and concatenates
/// the jumbo ciphertext once all fragments arrive.
pub struct ConfirmedReassembly {
    dst_conn_id: u64,
    total: u8,
    fragments: Vec<Option<Vec<u8>>>,
    received: usize,
    bytes: usize,
    first_header: Option<[u8; constants::SHORT_HEADER_LENGTH]>,
}

impl ConfirmedReassembly {
    /// Starts reassembly from the first observed fragment header.
    pub fn new(header: SessionConfirmedHeader) -> Result<Self, HandshakeError> {
        if header.frag_total() == 0
            || usize::from(header.frag_total()) > constants::MAX_SESSION_CONFIRMED_FRAGMENTS
        {
            return Err(HandshakeError::BadFragment);
        }
        let total = usize::from(header.frag_total());
        let mut fragments = Vec::with_capacity(total);
        fragments.resize_with(total, || None);
        Ok(Self {
            dst_conn_id: header.dst_conn_id(),
            total: header.frag_total(),
            fragments,
            received: 0,
            bytes: 0,
            first_header: None,
        })
    }

    /// Adds one deprotected fragment's payload bytes.
    pub fn add_fragment(
        &mut self,
        header: SessionConfirmedHeader,
        fragment: Vec<u8>,
    ) -> Result<(), HandshakeError> {
        if header.dst_conn_id() != self.dst_conn_id
            || header.frag_total() != self.total
            || header.frag_number() >= self.total
        {
            return Err(HandshakeError::BadFragment);
        }
        if fragment.is_empty() || fragment.len() > constants::MAX_HANDSHAKE_PAYLOAD_BYTES {
            return Err(HandshakeError::BadFragment);
        }
        let slot = &mut self.fragments[usize::from(header.frag_number())];
        if let Some(existing) = slot {
            if *existing != fragment {
                return Err(HandshakeError::DuplicateFragment);
            }
            return Ok(());
        }
        self.bytes = self
            .bytes
            .checked_add(fragment.len())
            .ok_or(HandshakeError::AggregateTooLarge)?;
        if self.bytes > constants::MAX_CONFIRMED_REASSEMBLED_BYTES {
            return Err(HandshakeError::AggregateTooLarge);
        }
        *slot = Some(fragment);
        self.received += 1;
        // Fragment 0's short header is the Noise AD for the static-key
        // frame regardless of arrival order, so it is pinned here.
        if header.frag_number() == 0 {
            self.first_header = Some(header.encode());
        }
        Ok(())
    }

    /// Returns whether all fragments arrived.
    pub fn is_complete(&self) -> bool {
        self.received == usize::from(self.total)
    }

    /// Returns fragment 0's short header bytes (the static-frame AD).
    pub fn first_header(&self) -> Option<[u8; constants::SHORT_HEADER_LENGTH]> {
        self.first_header
    }

    /// Concatenates the jumbo ciphertext (frag0 header is the Noise AD
    /// and is handled by the caller, not included here).
    pub fn reassemble(self) -> Result<Vec<u8>, HandshakeError> {
        if !self.is_complete() {
            return Err(HandshakeError::IncompleteFragments);
        }
        let mut jumbo = Vec::with_capacity(self.bytes);
        for fragment in self.fragments.into_iter().flatten() {
            jumbo.extend_from_slice(&fragment);
        }
        Ok(jumbo)
    }
}

/// Splits a reassembled SessionConfirmed jumbo into the 48-byte static
/// frame and the remaining payload ciphertext.
pub fn split_confirmed_jumbo(jumbo: &[u8]) -> Result<(&[u8], &[u8]), HandshakeError> {
    const STATIC_FRAME: usize = constants::KEY_LENGTH + constants::AUTH_TAG_LENGTH;
    if jumbo.len()
        < STATIC_FRAME + constants::MIN_HANDSHAKE_PAYLOAD_BYTES + constants::AUTH_TAG_LENGTH
    {
        return Err(HandshakeError::Truncated);
    }
    Ok((&jumbo[..STATIC_FRAME], &jumbo[STATIC_FRAME..]))
}

/// Freshness policy for RouterInfo establishment (local policy; the
/// wire carries only the published time).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterInfoFreshness {
    /// Local Unix time in seconds supplied by the caller.
    pub now_seconds: u64,
    /// Maximum accepted publication age in seconds.
    pub max_age_seconds: u64,
    /// Maximum accepted future skew in seconds.
    pub max_future_skew_seconds: u64,
}

impl RouterInfoFreshness {
    /// Builds the default establishment policy for one local timestamp.
    pub const fn default_for(now_seconds: u64) -> Self {
        Self {
            now_seconds,
            max_age_seconds: constants::ESTABLISHMENT_ROUTER_INFO_MAX_AGE_SECONDS,
            max_future_skew_seconds: constants::ESTABLISHMENT_ROUTER_INFO_MAX_FUTURE_SKEW_SECONDS,
        }
    }
}

/// The authenticated identity and SSU2 static key from RouterInfo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedPeer {
    /// Canonical RouterIdentity hash.
    pub router_hash: Hash,
    /// SSU2 static public key bound to the handshake transcript.
    pub transport_static_key: Ssu2PublicKey,
    /// The complete validated RouterInfo bytes for caller handoff.
    pub router_info: Vec<u8>,
}

/// Validates a complete RouterInfo for SSU2 establishment without
/// mutating NetDB: structural decode, signature, expected identity
/// hash, SSU2 address presence with `v=2`, static-key binding against
/// the authenticated handshake peer, intro-key shape where required,
/// and publication freshness.
pub fn validate_router_info(
    bytes: &[u8],
    maximum: usize,
    expected_hash: Option<Hash>,
    expected_static_key: &Ssu2PublicKey,
    freshness: RouterInfoFreshness,
) -> Result<AuthenticatedPeer, HandshakeError> {
    if bytes.is_empty()
        || bytes.len() > maximum
        || bytes.len() > constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES
    {
        return Err(HandshakeError::RouterInfoMalformed);
    }
    let info =
        RouterInfo::decode(bytes, maximum).map_err(|_| HandshakeError::RouterInfoMalformed)?;
    verify_router_info(&info).map_err(|_| HandshakeError::RouterInfoSignatureInvalid)?;
    let router_hash = router_identity_hash(info.router_identity())
        .map_err(|_| HandshakeError::RouterInfoMalformed)?;
    if expected_hash.is_some_and(|expected| expected != router_hash) {
        return Err(HandshakeError::PeerIdentityMismatch);
    }
    if info.router_identity().public_key().key_type() != CryptoKeyType::X25519 {
        return Err(HandshakeError::UnsupportedPeerKey);
    }
    let mut matched = false;
    for address in info.addresses() {
        if address.transport_style() != "SSU2" {
            continue;
        }
        let Some(version) = address.options().get("v") else {
            continue;
        };
        if !version.split(',').any(|value| value == "2") {
            continue;
        }
        let Some(encoded_key) = address.options().get("s") else {
            return Err(HandshakeError::TransportStaticKeyMismatch);
        };
        let key = decode_i2p_base64::<32>(encoded_key, "s")
            .map_err(|_| HandshakeError::TransportStaticKeyMismatch)?;
        if !crate::crypto::public_eq(&key, expected_static_key.as_bytes()) {
            return Err(HandshakeError::TransportStaticKeyMismatch);
        }
        if let Some(intro) = address.options().get("i") {
            let intro_key =
                decode_i2p_base64::<32>(intro, "i").map_err(|_| HandshakeError::IntroKeyInvalid)?;
            if intro_key.iter().all(|byte| *byte == 0) {
                return Err(HandshakeError::IntroKeyInvalid);
            }
        } else if address.options().get("host").is_some()
            || address.options().get("port").is_some()
            || address.options().get("ihost0").is_some()
        {
            return Err(HandshakeError::IntroKeyInvalid);
        }
        matched = true;
    }
    if !matched {
        return Err(HandshakeError::MissingSsu2Address);
    }
    let published = info.published().as_millis().div_ceil(1000);
    if published
        > freshness
            .now_seconds
            .saturating_add(freshness.max_future_skew_seconds)
    {
        return Err(HandshakeError::RouterInfoFuture);
    }
    if freshness.now_seconds.saturating_sub(published) > freshness.max_age_seconds {
        return Err(HandshakeError::RouterInfoStale);
    }
    Ok(AuthenticatedPeer {
        router_hash,
        transport_static_key: *expected_static_key,
        router_info: bytes.to_vec(),
    })
}

/// Builds a SessionConfirmed payload with the RouterInfo block first,
/// followed by optional padding. The RouterInfo block carries the
/// complete (single-fragment, `0x01`) RouterInfo; multi-fragment
/// SessionConfirmed datagrams fragment the encrypted jumbo, never the
/// block itself.
pub fn build_confirmed_payload(
    router_info: &[u8],
    padding: Vec<u8>,
) -> Result<Vec<u8>, HandshakeError> {
    if router_info.is_empty() || router_info.len() > constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES
    {
        return Err(HandshakeError::RouterInfoMalformed);
    }
    let mut blocks = Vec::with_capacity(2);
    blocks.push(Block::RouterInfo(RouterInfoBlock::new(
        0,
        router_info.to_vec(),
    )?));
    if !padding.is_empty() {
        blocks.push(Block::Padding(PaddingBlock::new(padding)?));
    }
    let payload = encode_blocks(blocks)?;
    if payload.len() > constants::MAX_HANDSHAKE_PAYLOAD_BYTES {
        return Err(HandshakeError::TooLong);
    }
    Ok(payload)
}

/// Requires the first block of a SessionConfirmed payload to be the
/// RouterInfo block and returns a copy of its bytes.
pub fn require_first_router_info(payload: &[u8]) -> Result<Vec<u8>, HandshakeError> {
    let parsed = parse_blocks(payload)?;
    let mut blocks = parsed.blocks().iter();
    match blocks.next() {
        Some(DecodedBlock::RouterInfo(block)) => Ok(block.encoded().to_vec()),
        Some(_) => Err(HandshakeError::RouterInfoNotFirst),
        None => Err(HandshakeError::RouterInfoMalformed),
    }
}

/// Extracts and skew-classifies the first DateTime block of a payload.
pub fn require_timestamp(payload: &[u8]) -> Result<u32, HandshakeError> {
    let parsed: ParsedBlocks<'_> =
        parse_blocks(payload).map_err(|_| HandshakeError::MissingTimestamp)?;
    for block in parsed.blocks() {
        if let DecodedBlock::Timestamp(value) = block {
            return Ok(value.seconds());
        }
    }
    Err(HandshakeError::MissingTimestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::PaddingBlock;

    fn intro() -> IntroKey {
        IntroKey::new([0x42_u8; 32])
    }

    fn endpoint() -> crate::address::Ssu2Endpoint {
        crate::address::Ssu2Endpoint::new("192.0.2.1".parse().expect("test ip"), 44000)
            .expect("endpoint")
    }

    #[test]
    fn token_request_round_trip_with_skew_policy() {
        let skew = ClockSkewPolicy::handshake();
        let datagram = build_token_request(&intro(), 111, 222, 7, 1_700_000_000, vec![9_u8; 16])
            .expect("build");
        assert!(datagram.len() >= constants::MIN_DATAGRAM_LENGTH);
        let mut inbound = datagram.clone();
        let parsed =
            parse_token_request(&mut inbound, &intro(), skew, 1_700_000_060).expect("parse");
        assert_eq!(parsed.timestamp(), 1_700_000_000);
        assert_eq!(parsed.header().src_conn_id(), 111);

        let mut stale = datagram.clone();
        assert_eq!(
            parse_token_request(&mut stale, &intro(), skew, 1_700_000_000 + 10_000).map(|_| ()),
            Err(HandshakeError::StaleTimestamp)
        );
        let mut future = datagram;
        assert_eq!(
            parse_token_request(&mut future, &intro(), skew, 1_699_000_000).map(|_| ()),
            Err(HandshakeError::FutureTimestamp)
        );
    }

    #[test]
    fn token_request_rejects_wrong_intro_key_and_truncation() {
        let datagram = build_token_request(&intro(), 111, 222, 7, 1_700_000_000, vec![9_u8; 16])
            .expect("build");
        let wrong = IntroKey::new([0x43_u8; 32]);
        let mut inbound = datagram.clone();
        assert!(
            parse_token_request(
                &mut inbound,
                &wrong,
                ClockSkewPolicy::handshake(),
                1_700_000_000
            )
            .is_err()
        );
        let mut short = datagram[..datagram.len() - 1].to_vec();
        assert!(
            parse_token_request(
                &mut short,
                &intro(),
                ClockSkewPolicy::handshake(),
                1_700_000_000
            )
            .is_err()
        );
    }

    #[test]
    fn retry_round_trip_and_amplification_budget() {
        let skew = ClockSkewPolicy::handshake();
        let address = AddressBlock::new(endpoint());
        let datagram = build_retry(
            &intro(),
            80,
            111,
            222,
            9,
            0x0102_0304_0506_0708,
            1_700_000_000,
            address,
            None,
            vec![1_u8; 16],
        )
        .expect("build");
        assert!(datagram.len() <= 3 * 80);
        let mut inbound = datagram.clone();
        let parsed = parse_retry(&mut inbound, &intro(), skew, 1_700_000_060).expect("parse");
        assert_eq!(parsed.token(), 0x0102_0304_0506_0708);
        assert!(parsed.termination().is_none());

        assert_eq!(
            build_retry(
                &intro(),
                80,
                111,
                222,
                9,
                0,
                1_700_000_000,
                AddressBlock::new(endpoint()),
                None,
                Vec::new(),
            )
            .map(|_| ()),
            Err(HandshakeError::TokenRejected)
        );
        assert_eq!(
            build_retry(
                &intro(),
                40,
                111,
                222,
                9,
                5,
                1_700_000_000,
                AddressBlock::new(endpoint()),
                None,
                vec![1_u8; 64],
            )
            .map(|_| ()),
            Err(HandshakeError::AmplificationExceeded)
        );
    }

    #[test]
    fn retry_termination_allows_zero_token() {
        let termination = TerminationBlock::new(0, crate::block::TerminationReason::ClockSkew);
        let datagram = build_retry(
            &intro(),
            200,
            111,
            222,
            9,
            0,
            1_700_000_000,
            AddressBlock::new(endpoint()),
            Some(termination),
            Vec::new(),
        )
        .expect("build");
        let mut inbound = datagram;
        let parsed = parse_retry(
            &mut inbound,
            &intro(),
            ClockSkewPolicy::handshake(),
            1_700_000_000,
        )
        .expect("parse");
        assert_eq!(parsed.token(), 0);
        assert!(parsed.termination().is_some());
    }

    #[test]
    fn session_request_build_parse_round_trip() {
        let header = LongHeader::new(222, 7, MessageType::SessionRequest, 111, 0).expect("header");
        let ephemeral = Ssu2PublicKey::from_bytes_for_test([7_u8; 32]);
        let payload = vec![0xaa_u8; 48];
        let datagram = build_session_request(
            &header,
            &ephemeral,
            &payload,
            intro().as_bytes(),
            intro().as_bytes(),
        )
        .expect("build");
        let mut inbound = datagram;
        let parsed = parse_session_request(&mut inbound, &intro()).expect("parse");
        assert_eq!(parsed.header, header);
        assert_eq!(parsed.ephemeral, ephemeral);
        assert_eq!(parsed.ciphertext, payload);
    }

    #[test]
    fn confirmed_fragments_reassemble_exactly() {
        let jumbo = vec![0x5au8; 3000];
        let k2 = [0x77_u8; 32];
        let datagrams =
            build_session_confirmed(999, &jumbo, 1000, intro().as_bytes(), &k2).expect("build");
        assert_eq!(datagrams.len(), 3);
        let mut reassembly = None;
        let mut payloads = Vec::new();
        for mut datagram in datagrams {
            crate::crypto::remove_header_protection(
                &mut datagram,
                constants::SHORT_HEADER_LENGTH,
                intro().as_bytes(),
                &k2,
                false,
            )
            .expect("unprotect");
            let header = SessionConfirmedHeader::decode(&datagram[..16]).expect("header");
            if reassembly.is_none() {
                reassembly = Some(ConfirmedReassembly::new(header).expect("reassembly"));
            }
            payloads.push((header, datagram[16..].to_vec()));
        }
        let mut reassembly = reassembly.expect("started");
        for (header, payload) in payloads {
            reassembly.add_fragment(header, payload).expect("add");
        }
        assert!(reassembly.is_complete());
        assert_eq!(reassembly.reassemble().expect("jumbo"), jumbo);
    }

    #[test]
    fn confirmed_reassembly_rejects_duplicates_and_gaps() {
        let jumbo = vec![0x5au8; 200];
        let k2 = [0x77_u8; 32];
        let datagrams =
            build_session_confirmed(999, &jumbo, 100, intro().as_bytes(), &k2).expect("build");
        assert_eq!(datagrams.len(), 2);
        let mut decoded = Vec::new();
        for mut datagram in datagrams {
            crate::crypto::remove_header_protection(
                &mut datagram,
                constants::SHORT_HEADER_LENGTH,
                intro().as_bytes(),
                &k2,
                false,
            )
            .expect("unprotect");
            let header = SessionConfirmedHeader::decode(&datagram[..16]).expect("header");
            decoded.push((header, datagram[16..].to_vec()));
        }
        let mut reassembly = ConfirmedReassembly::new(decoded[0].0).expect("reassembly");
        reassembly
            .add_fragment(decoded[0].0, decoded[0].1.clone())
            .expect("add");
        reassembly
            .add_fragment(decoded[0].0, decoded[0].1.clone())
            .expect("idempotent");
        assert_eq!(
            reassembly
                .add_fragment(decoded[0].0, vec![1_u8; 100])
                .map(|_| ()),
            Err(HandshakeError::DuplicateFragment)
        );
        assert_eq!(
            reassembly.reassemble().map(|_| ()),
            Err(HandshakeError::IncompleteFragments)
        );
    }

    #[test]
    fn replay_cache_bounds_duplicates_and_capacity() {
        let mut cache = HandshakeReplayCache::new(2, 240).expect("cache");
        let first = ReplayToken::from_ephemeral_bytes(&[1_u8; 32]);
        let second = ReplayToken::from_ephemeral_bytes(&[2_u8; 32]);
        let third = ReplayToken::from_ephemeral_bytes(&[3_u8; 32]);
        assert_eq!(cache.check_and_record(first, 100), ReplayDecision::Fresh);
        assert_eq!(cache.check_and_record(first, 101), ReplayDecision::Replayed);
        assert_eq!(cache.check_and_record(second, 102), ReplayDecision::Fresh);
        assert_eq!(
            cache.check_and_record(third, 103),
            ReplayDecision::CacheFull
        );
        assert_eq!(
            cache.check_and_record(first, 100 + 240),
            ReplayDecision::Fresh
        );
    }

    #[test]
    fn prevalidation_drops_cheap_invalid_datagrams() {
        let mut short = vec![0_u8; 20];
        assert!(
            prevalidate_long_datagram(&mut short, &intro(), MessageType::SessionRequest, true)
                .is_err()
        );
        let header = LongHeader::new(222, 7, MessageType::SessionRequest, 111, 0).expect("header");
        let mut datagram = header.encode().to_vec();
        datagram.extend(vec![0_u8; 56]);
        crate::crypto::apply_header_protection(
            &mut datagram,
            constants::LONG_HEADER_LENGTH,
            intro().as_bytes(),
            intro().as_bytes(),
            true,
        )
        .expect("protect");
        let pre = prevalidate_long_datagram(
            &mut datagram.clone(),
            &intro(),
            MessageType::SessionRequest,
            true,
        )
        .expect("prevalidate");
        assert_eq!(pre.header(), header);
        assert!(
            prevalidate_long_datagram(&mut datagram, &intro(), MessageType::Retry, false).is_err()
        );
    }

    #[test]
    fn padding_helper_reaches_minimum_payload() {
        let padding = PaddingBlock::new(vec![0_u8; 4]).expect("padding");
        assert_eq!(padding.len(), 4);
    }
}
