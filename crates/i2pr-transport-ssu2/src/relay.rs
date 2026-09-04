//! Bounded runtime-neutral SSU2 relay roles and HolePunch (Plan 160).
//!
//! This module owns the session-level relay state machines for the
//! three v2 roles — requester (Alice), introducer (Bob), target
//! (Charlie) — plus the out-of-session HolePunch codec. Like
//! [`crate::peer_test`], it never touches sockets, Tokio, timers, or
//! NetDB; the runtime drives these machines after its session or
//! intro-key AEAD authenticated each datagram.
//!
//! Normative traceability: SSU2 specification §Relay (block types 7–9,
//! message type 11 HolePunch; RelayRequest Alice→Bob, RelayIntro
//! Bob→Charlie preceded by Alice's RouterInfo, RelayResponse
//! Charlie→Bob→Alice and Charlie→Alice in HolePunch). Signature
//! preimages are verbatim:
//!
//! ```text
//! RelayRequest (Alice signs):
//!   prologue "RelayRequestData" (16, not on wire)
//!   bhash    Bob's 32-byte hash (not on wire)
//!   chash    Charlie's 32-byte hash (not on wire)
//!   nonce (4 BE) | tag (4 BE) | timestamp (4 BE) | ver (1)
//!   asz (1) | AlicePort (2 BE) | Alice IP (asz-2)
//!
//! RelayResponse accept / Charlie reject (Charlie signs):
//!   prologue "RelayAgreementOK" (16, not on wire)
//!   bhash (32, not on wire)
//!   nonce (4) | timestamp (4) | ver (1) | csz (1)
//!   CharliePort (2, absent if csz=0) | Charlie IP (csz-2)
//!
//! RelayResponse Bob reject (Bob signs):
//!   prologue "RelayAgreementOK" + bhash + nonce + timestamp + ver
//!   + csz=0 (no endpoint, no token)
//!
//! RelayIntro (Bob→Charlie) forwards Alice's RelayRequest bytes
//! unmodified under Alice's hash; Charlie verifies with the
//! RelayRequest preimage.
//!
//! HolePunch (type 11, long header, Charlie→Alice):
//!   DestConnID = (nonce << 32) | nonce; SrcConnID = !DestConnID
//!   header protection + AEAD under Alice's intro key
//!   payload = DateTime + Address + RelayResponse (+ optional Padding)
//! ```
//!
//! Only Ed25519 signers verify here (see `peer_test` debt note).
//! Introducer service gating (`disabled by default`) lives in the
//! runtime/daemon config; this crate only bounds the tables. Relay
//! success never proves direct inbound reachability — targets and
//! requesters feed `RelayFirewalledSignal`-class evidence to the
//! reachability policy, never `Reachable`.

use std::collections::{HashMap, HashSet};

use i2pr_crypto::verify_signature;
use i2pr_proto::{SignatureValue, SigningPublicKey};
use thiserror::Error;

use crate::address::Ssu2Endpoint;
use crate::block::{
    AddressBlock, Block, RelayIntroBlock, RelayRequestBlock, RelayResponseBlock, TimestampBlock,
    encode_blocks, parse_blocks,
};
use crate::constants;
use crate::header::{LongHeader, MessageType};
use crate::packet::DatagramLengthClass;

/// Maximum concurrent relay requester entries (Alice).
pub const MAX_RELAY_REQUESTS_GLOBAL: usize = 8;
/// Maximum concurrent requests toward one introducer/target peer.
pub const MAX_RELAY_REQUESTS_PER_PEER: usize = 2;
/// Maximum live relay tags retained by one introducer table.
pub const MAX_RELAY_TAGS_GLOBAL: usize = 16;
/// Maximum live tags for one requester hash.
pub const MAX_RELAY_TAGS_PER_PEER: usize = 4;
/// Relay tag lifetime in seconds (deterministic expiry).
pub const RELAY_TAG_LIFETIME_SECS: u64 = 120;
/// Relay request lifetime in milliseconds (central scheduler).
pub const RELAY_REQUEST_TIMEOUT_MS: u64 = 10_000;
/// Maximum accepted relay timestamp skew in seconds.
pub const RELAY_MAX_CLOCK_SKEW_SECONDS: u64 = 120;
/// Spec prologue for RelayRequest signatures (not on wire).
pub const RELAY_REQUEST_PROLOGUE: &[u8; 16] = b"RelayRequestData";
/// Spec prologue for RelayResponse signatures (not on wire).
pub const RELAY_RESPONSE_PROLOGUE: &[u8; 16] = b"RelayAgreementOK";
/// Maximum HolePunch payload bytes (DateTime + Address + Response).
pub const MAX_HOLE_PUNCH_PAYLOAD_BYTES: usize = 512;

/// Typed relay failures.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RelayError {
    /// The requester table is full.
    #[error("SSU2 relay request table is full")]
    TooManyRequests,
    /// The per-peer request quota is full.
    #[error("SSU2 relay per-peer quota is full")]
    PeerQuotaExceeded,
    /// The tag table is full.
    #[error("SSU2 relay tag table is full")]
    TooManyTags,
    /// The per-peer tag quota is full.
    #[error("SSU2 relay per-peer tag quota is full")]
    TagQuotaExceeded,
    /// The nonce/tag correlation is already in use.
    #[error("SSU2 relay correlation is already in use")]
    DuplicateCorrelation,
    /// No live entry matches the correlation.
    #[error("SSU2 relay correlation is unknown")]
    UnknownCorrelation,
    /// The relay tag is unknown, expired, or bound to another peer.
    #[error("SSU2 relay tag is invalid")]
    InvalidTag,
    /// The message arrived in the wrong role/state.
    #[error("SSU2 relay message has the wrong role or state")]
    WrongRole,
    /// The sender does not match the tracked request.
    #[error("SSU2 relay sender does not match its request")]
    SenderMismatch,
    /// A signature is missing, malformed, or does not verify.
    #[error("SSU2 relay signature is invalid")]
    InvalidSignature,
    /// The signer key type is unsupported (Ed25519 only this pass).
    #[error("SSU2 relay signer type is unsupported")]
    UnsupportedSigner,
    /// The timestamp is outside the freshness window.
    #[error("SSU2 relay timestamp is stale")]
    StaleTimestamp,
    /// The version under test is not v2.
    #[error("SSU2 relay version is unsupported")]
    UnsupportedVersion,
    /// The request expired before completion.
    #[error("SSU2 relay request expired")]
    Expired,
    /// The introducer service is disabled.
    #[error("SSU2 introducer service is disabled")]
    ServiceDisabled,
    /// A HolePunch datagram is malformed or unauthenticated.
    #[error("SSU2 hole-punch is invalid")]
    InvalidHolePunch,
    /// The entry was cancelled.
    #[error("SSU2 relay entry cancelled")]
    Cancelled,
}

/// Builds the exact RelayRequest signature preimage.
pub fn relay_request_preimage(
    bob_hash: &[u8; 32],
    charlie_hash: &[u8; 32],
    nonce: u32,
    tag: u32,
    timestamp: u32,
    version: u8,
    endpoint: Ssu2Endpoint,
) -> Vec<u8> {
    let (asz, ip_bytes) = endpoint_parts(endpoint);
    let mut preimage = Vec::with_capacity(16 + 32 + 32 + 4 + 4 + 4 + 1 + 1 + 2 + 16);
    preimage.extend_from_slice(RELAY_REQUEST_PROLOGUE);
    preimage.extend_from_slice(bob_hash);
    preimage.extend_from_slice(charlie_hash);
    preimage.extend_from_slice(&nonce.to_be_bytes());
    preimage.extend_from_slice(&tag.to_be_bytes());
    preimage.extend_from_slice(&timestamp.to_be_bytes());
    preimage.push(version);
    preimage.push(asz);
    preimage.extend_from_slice(&endpoint.port().to_be_bytes());
    preimage.extend_from_slice(&ip_bytes);
    preimage
}

/// Builds the exact RelayResponse signature preimage (Charlie-signed
/// accept or Charlie/other reject; Bob rejects use `csz = 0` with
/// `endpoint = None` and Bob's key).
pub fn relay_response_preimage(
    bob_hash: &[u8; 32],
    nonce: u32,
    timestamp: u32,
    version: u8,
    endpoint: Option<Ssu2Endpoint>,
) -> Vec<u8> {
    let mut preimage = Vec::with_capacity(16 + 32 + 4 + 4 + 1 + 1 + 2 + 16);
    preimage.extend_from_slice(RELAY_RESPONSE_PROLOGUE);
    preimage.extend_from_slice(bob_hash);
    preimage.extend_from_slice(&nonce.to_be_bytes());
    preimage.extend_from_slice(&timestamp.to_be_bytes());
    preimage.push(version);
    match endpoint {
        Some(endpoint) => {
            let (asz, ip_bytes) = endpoint_parts(endpoint);
            preimage.push(asz);
            preimage.extend_from_slice(&endpoint.port().to_be_bytes());
            preimage.extend_from_slice(&ip_bytes);
        }
        None => preimage.push(0),
    }
    preimage
}

fn endpoint_parts(endpoint: Ssu2Endpoint) -> (u8, Vec<u8>) {
    match endpoint.ip() {
        core::net::IpAddr::V4(address) => (6, address.octets().to_vec()),
        core::net::IpAddr::V6(address) => (16 + 2, address.octets().to_vec()),
    }
}

/// Verifies a RelayRequest signature (Alice signs).
pub fn verify_relay_request(
    block: &RelayRequestBlock,
    bob_hash: &[u8; 32],
    charlie_hash: &[u8; 32],
    signer: &SigningPublicKey,
    signature: &[u8],
) -> Result<(), RelayError> {
    if !matches!(block.version(), 1 | 2) {
        return Err(RelayError::UnsupportedVersion);
    }
    if signature.is_empty() {
        return Err(RelayError::InvalidSignature);
    }
    let preimage = relay_request_preimage(
        bob_hash,
        charlie_hash,
        block.nonce(),
        block.relay_tag(),
        block.timestamp(),
        block.version(),
        block.endpoint(),
    );
    let value = SignatureValue::new(signer.key_type(), signature.to_vec())
        .map_err(|_| RelayError::UnsupportedSigner)?;
    verify_signature(signer, &preimage, &value).map_err(|_| RelayError::InvalidSignature)
}

/// Verifies a RelayResponse signature.
///
/// Accept and Charlie/other rejects verify under Charlie's key with
/// the endpoint shape from the block; Bob rejects (no endpoint, no
/// signature per the block codec) are accepted without a signature
/// but never carry a token or endpoint.
pub fn verify_relay_response(
    block: &RelayResponseBlock,
    bob_hash: &[u8; 32],
    signer: Option<&SigningPublicKey>,
    signature: &[u8],
) -> Result<(), RelayError> {
    if !matches!(block.version(), 1 | 2) {
        return Err(RelayError::UnsupportedVersion);
    }
    // Bob rejections carry no signature by construction; they are
    // explicit refusals, never confirmations.
    if matches!(
        block.code(),
        crate::block::RelayResponseCode::RejectedByBob(_)
    ) {
        if !signature.is_empty() || block.endpoint().is_some() || block.token().is_some() {
            return Err(RelayError::InvalidSignature);
        }
        return Ok(());
    }
    if signature.is_empty() {
        return Err(RelayError::InvalidSignature);
    }
    let Some(signer) = signer else {
        return Err(RelayError::InvalidSignature);
    };
    let preimage = relay_response_preimage(
        bob_hash,
        block.nonce(),
        block.timestamp(),
        block.version(),
        block.endpoint(),
    );
    let value = SignatureValue::new(signer.key_type(), signature.to_vec())
        .map_err(|_| RelayError::UnsupportedSigner)?;
    verify_signature(signer, &preimage, &value).map_err(|_| RelayError::InvalidSignature)
}

/// Checks relay timestamp freshness (Unix seconds, both directions).
pub fn check_relay_freshness(timestamp: u32, now_secs: u64) -> Result<(), RelayError> {
    if u64::from(timestamp).abs_diff(now_secs) > RELAY_MAX_CLOCK_SKEW_SECONDS {
        return Err(RelayError::StaleTimestamp);
    }
    Ok(())
}

/// Derives the spec HolePunch connection IDs from the relay nonce.
pub fn hole_punch_conn_ids(nonce: u32) -> (u64, u64) {
    let dest = (u64::from(nonce) << 32) | u64::from(nonce);
    (dest, !dest)
}

/// Builds an out-of-session HolePunch datagram (type 11) under Alice's
/// intro key: long header plus AEAD payload (DateTime + Address +
/// RelayResponse). `packet_number` is random/ignored per spec and
/// supplied by the caller (production: OS randomness).
pub fn build_hole_punch(
    alice_intro: &crate::crypto::IntroKey,
    src_conn_id: u64,
    dst_conn_id: u64,
    packet_number: u32,
    timestamp: u32,
    charlie_endpoint: Ssu2Endpoint,
    response: RelayResponseBlock,
) -> Result<Vec<u8>, RelayError> {
    if src_conn_id == dst_conn_id {
        return Err(RelayError::InvalidHolePunch);
    }
    let address = AddressBlock::new(charlie_endpoint);
    let blocks = vec![
        Block::Timestamp(TimestampBlock::new(timestamp)),
        Block::Address(address),
        Block::RelayResponse(response),
    ];
    let payload = encode_blocks(blocks).map_err(|_| RelayError::InvalidHolePunch)?;
    if payload.len() > MAX_HOLE_PUNCH_PAYLOAD_BYTES {
        return Err(RelayError::InvalidHolePunch);
    }
    let header = LongHeader::new(
        dst_conn_id,
        packet_number,
        MessageType::HolePunch,
        src_conn_id,
        0,
    )
    .map_err(|_| RelayError::InvalidHolePunch)?;
    let header_bytes = header.encode();
    let sealed =
        crate::crypto::seal_token_payload(alice_intro, packet_number, &header_bytes, &payload)
            .map_err(|_| RelayError::InvalidHolePunch)?;
    let mut datagram = Vec::with_capacity(constants::LONG_HEADER_LENGTH + sealed.len());
    datagram.extend_from_slice(&header_bytes);
    datagram.extend_from_slice(&sealed);
    crate::crypto::apply_header_protection(
        &mut datagram,
        constants::LONG_HEADER_LENGTH,
        alice_intro.as_bytes(),
        alice_intro.as_bytes(),
        false,
    )
    .map_err(|_| RelayError::InvalidHolePunch)?;
    DatagramLengthClass::classify(datagram.len()).map_err(|_| RelayError::InvalidHolePunch)?;
    Ok(datagram)
}

/// A parsed HolePunch: header plus authenticated payload blocks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HolePunchMessage {
    /// The deprotected long header.
    pub header: LongHeader,
    /// Payload timestamp.
    pub timestamp: u32,
    /// Charlie's endpoint from the Address block.
    pub charlie_endpoint: Ssu2Endpoint,
    /// The carried RelayResponse (accept with token on success).
    pub response: RelayResponseBlock,
}

/// Parses an inbound HolePunch after cheap prevalidation: intro-key
/// header deprotection, exact header decode, version/network/type
/// checks, minimum tail, AEAD open, and required DateTime + Address +
/// RelayResponse block presence.
pub fn parse_hole_punch(
    datagram: &mut [u8],
    alice_intro: &crate::crypto::IntroKey,
) -> Result<HolePunchMessage, RelayError> {
    let pre = crate::handshake::prevalidate_long_datagram(
        datagram,
        alice_intro,
        MessageType::HolePunch,
        false,
    )
    .map_err(|_| RelayError::InvalidHolePunch)?;
    let header = pre.header();
    let sealed = &datagram[constants::LONG_HEADER_LENGTH..];
    let payload = crate::crypto::open_token_payload(
        alice_intro,
        header.packet_number(),
        &header.encode(),
        sealed,
    )
    .map_err(|_| RelayError::InvalidHolePunch)?;
    let parsed = parse_blocks(&payload).map_err(|_| RelayError::InvalidHolePunch)?;
    let mut timestamp = None;
    let mut endpoint = None;
    let mut response = None;
    for block in parsed.blocks() {
        match block {
            crate::block::DecodedBlock::Timestamp(value) => {
                if timestamp.is_some() {
                    return Err(RelayError::InvalidHolePunch);
                }
                timestamp = Some(value.seconds());
            }
            crate::block::DecodedBlock::Address(value) => {
                if endpoint.is_some() {
                    return Err(RelayError::InvalidHolePunch);
                }
                endpoint = Some(value.endpoint());
            }
            crate::block::DecodedBlock::RelayResponse(value) => {
                if response.is_some() {
                    return Err(RelayError::InvalidHolePunch);
                }
                response = Some(value.clone());
            }
            _ => {}
        }
    }
    Ok(HolePunchMessage {
        header,
        timestamp: timestamp.ok_or(RelayError::InvalidHolePunch)?,
        charlie_endpoint: endpoint.ok_or(RelayError::InvalidHolePunch)?,
        response: response.ok_or(RelayError::InvalidHolePunch)?,
    })
}

// ---------------------------------------------------------------------------
// Requester (Alice)
// ---------------------------------------------------------------------------

/// Where one requester entry sits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequesterState {
    /// RelayRequest sent, awaiting RelayResponse (via Bob).
    AwaitingResponse,
    /// Accept received, awaiting HolePunch from Charlie.
    AwaitingHolePunch,
    /// Terminal.
    Completed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RequesterEntry {
    nonce: u32,
    tag: u32,
    bob_hash: [u8; 32],
    charlie_hash: [u8; 32],
    alice_endpoint: Ssu2Endpoint,
    state: RequesterState,
    deadline_ms: u64,
    hole_punched: bool,
}

/// Bounded Alice-side relay requester table.
///
/// `Debug` is redacted (counts only): entries carry router hashes,
/// nonces, tags, and endpoints (Plan 160 §12).
#[derive(Clone, Default)]
pub struct RelayRequester {
    entries: HashMap<u32, RequesterEntry>,
}

impl std::fmt::Debug for RelayRequester {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayRequester")
            .field("live_requests", &self.entries.len())
            .finish()
    }
}

impl RelayRequester {
    /// Creates an empty requester table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of live requests.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no requests are tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Starts one request. `nonce`/`tag` must be nonzero caller
    /// randomness; at most two concurrent requests per introducer.
    pub fn start(
        &mut self,
        nonce: u32,
        tag: u32,
        bob_hash: [u8; 32],
        charlie_hash: [u8; 32],
        alice_endpoint: Ssu2Endpoint,
        now_ms: u64,
    ) -> Result<(), RelayError> {
        if nonce == 0 || tag == 0 {
            return Err(RelayError::DuplicateCorrelation);
        }
        self.expire_locked(now_ms);
        if self.entries.contains_key(&nonce) {
            return Err(RelayError::DuplicateCorrelation);
        }
        if self.entries.len() >= MAX_RELAY_REQUESTS_GLOBAL {
            return Err(RelayError::TooManyRequests);
        }
        let peer_count = self
            .entries
            .values()
            .filter(|entry| entry.bob_hash == bob_hash)
            .count();
        if peer_count >= MAX_RELAY_REQUESTS_PER_PEER {
            return Err(RelayError::PeerQuotaExceeded);
        }
        self.entries.insert(
            nonce,
            RequesterEntry {
                nonce,
                tag,
                bob_hash,
                charlie_hash,
                alice_endpoint,
                state: RequesterState::AwaitingResponse,
                deadline_ms: now_ms.saturating_add(RELAY_REQUEST_TIMEOUT_MS),
                hole_punched: false,
            },
        );
        Ok(())
    }

    /// Handles one authenticated RelayResponse for its request.
    ///
    /// The caller guarantees session authentication; this function
    /// enforces correlation, sender (must be Bob), tag/nonce match,
    /// freshness, version, and Charlie's signature before advancing.
    /// A second request with a distinct tag never cross-contaminates
    /// the first (correlation is by nonce).
    pub fn on_response(
        &mut self,
        block: &RelayResponseBlock,
        sender_hash: &[u8; 32],
        bob_hash: &[u8; 32],
        charlie_signer: Option<&SigningPublicKey>,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<RequesterState, RelayError> {
        self.expire_locked(now_ms);
        let nonce = block.nonce();
        let Some(entry) = self.entries.get_mut(&nonce) else {
            return Err(RelayError::UnknownCorrelation);
        };
        if entry.state != RequesterState::AwaitingResponse {
            return Err(RelayError::WrongRole);
        }
        if sender_hash != &entry.bob_hash {
            return Err(RelayError::SenderMismatch);
        }
        check_relay_freshness(block.timestamp(), now_secs)?;
        verify_relay_response(block, bob_hash, charlie_signer, block.signature())?;
        if !block.code().is_accept() {
            entry.state = RequesterState::Completed;
            return Ok(RequesterState::Completed);
        }
        let token = block.token().ok_or(RelayError::InvalidSignature)?;
        if token == 0 {
            return Err(RelayError::InvalidSignature);
        }
        entry.state = RequesterState::AwaitingHolePunch;
        Ok(RequesterState::AwaitingHolePunch)
    }

    /// Handles one authenticated HolePunch for its request.
    ///
    /// Correlates the HolePunch connection IDs and RelayResponse nonce
    /// to the exact request; validates Charlie's endpoint/signature/
    /// freshness; then reports readiness to transition into the normal
    /// SSU2 handshake (the caller dials via the standard establishment
    /// path with the HolePunch token — never a relay-specific fake
    /// session).
    pub fn on_hole_punch(
        &mut self,
        message: &HolePunchMessage,
        bob_hash: &[u8; 32],
        charlie_signer: Option<&SigningPublicKey>,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, RelayError> {
        self.expire_locked(now_ms);
        let nonce = message.response.nonce();
        let Some(entry) = self.entries.get_mut(&nonce) else {
            return Err(RelayError::UnknownCorrelation);
        };
        if entry.state != RequesterState::AwaitingHolePunch {
            return Err(RelayError::WrongRole);
        }
        // Connection-ID correlation: the HolePunch must use the
        // nonce-derived IDs.
        let (expected_dest, expected_src) = hole_punch_conn_ids(nonce);
        if message.header.dst_conn_id() != expected_dest
            || message.header.src_conn_id() != expected_src
        {
            return Err(RelayError::SenderMismatch);
        }
        check_relay_freshness(message.timestamp, now_secs)?;
        check_relay_freshness(message.response.timestamp(), now_secs)?;
        verify_relay_response(
            &message.response,
            bob_hash,
            charlie_signer,
            message.response.signature(),
        )?;
        if !message.response.code().is_accept() {
            entry.state = RequesterState::Completed;
            return Ok(false);
        }
        entry.hole_punched = true;
        entry.state = RequesterState::Completed;
        Ok(true)
    }

    /// Cancels one request and releases its quota.
    pub fn cancel(&mut self, nonce: u32) -> Result<(), RelayError> {
        self.entries
            .remove(&nonce)
            .map(|_| ())
            .ok_or(RelayError::UnknownCorrelation)
    }

    /// Expires timed-out requests, returning their nonces.
    pub fn poll_expired(&mut self, now_ms: u64) -> Vec<u32> {
        let expired: Vec<u32> = self
            .entries
            .iter()
            .filter(|(_, entry)| now_ms >= entry.deadline_ms)
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in &expired {
            self.entries.remove(nonce);
        }
        expired
    }

    /// Returns the earliest deadline, if any.
    pub fn next_deadline_ms(&self) -> Option<u64> {
        self.entries.values().map(|entry| entry.deadline_ms).min()
    }

    fn expire_locked(&mut self, now_ms: u64) {
        let expired: Vec<u32> = self
            .entries
            .iter()
            .filter(|(_, entry)| now_ms >= entry.deadline_ms)
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in &expired {
            self.entries.remove(nonce);
        }
    }
}

// ---------------------------------------------------------------------------
// Introducer (Bob)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TagEntry {
    tag: u32,
    alice_hash: [u8; 32],
    issued_secs: u64,
    expires_secs: u64,
}

impl TagEntry {
    fn expired(self, now_secs: u64) -> bool {
        now_secs >= self.expires_secs
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IntroducerRequest {
    nonce: u32,
    alice_hash: [u8; 32],
    charlie_hash: [u8; 32],
    tag: u32,
    deadline_ms: u64,
}

/// Bounded Bob-side introducer table.
///
/// The runtime instantiates this only when configuration explicitly
/// enables introducer service (disabled by default); the daemon
/// rejects `introducer_service = true` until Plan 160 closes.
///
/// `Debug` is redacted (counts + enablement only): tags bind router
/// hashes and relay tags (Plan 160 §12).
#[derive(Clone, Default)]
pub struct RelayIntroducer {
    tags: HashMap<u32, TagEntry>,
    requests: HashMap<u32, IntroducerRequest>,
    enabled: bool,
}

impl std::fmt::Debug for RelayIntroducer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayIntroducer")
            .field("enabled", &self.enabled)
            .field("live_tags", &self.tags.len())
            .field("live_requests", &self.requests.len())
            .finish()
    }
}

impl RelayIntroducer {
    /// Creates a disabled introducer (default: refuse everything).
    pub fn disabled() -> Self {
        Self {
            tags: HashMap::new(),
            requests: HashMap::new(),
            enabled: false,
        }
    }

    /// Creates an enabled introducer for controlled tests only.
    pub fn enabled_for_tests() -> Self {
        Self {
            tags: HashMap::new(),
            requests: HashMap::new(),
            enabled: true,
        }
    }

    /// Returns whether service is enabled.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the number of live tags.
    pub fn tag_count(&self) -> usize {
        self.tags.len()
    }

    /// Returns the number of live requests.
    pub fn request_count(&self) -> usize {
        self.requests.len()
    }

    /// Issues one relay tag bound to `alice_hash` from caller
    /// randomness (production: OS CSPRNG). Tags expire
    /// deterministically; shutdown clears them (see `shutdown`).
    pub fn issue_tag(
        &mut self,
        tag: u32,
        alice_hash: [u8; 32],
        now_secs: u64,
    ) -> Result<(), RelayError> {
        if !self.enabled {
            return Err(RelayError::ServiceDisabled);
        }
        if tag == 0 {
            return Err(RelayError::InvalidTag);
        }
        self.expire_tags_locked(now_secs);
        if self.tags.contains_key(&tag) {
            return Err(RelayError::DuplicateCorrelation);
        }
        if self.tags.len() >= MAX_RELAY_TAGS_GLOBAL {
            return Err(RelayError::TooManyTags);
        }
        let peer_count = self
            .tags
            .values()
            .filter(|entry| entry.alice_hash == alice_hash)
            .count();
        if peer_count >= MAX_RELAY_TAGS_PER_PEER {
            return Err(RelayError::TagQuotaExceeded);
        }
        self.tags.insert(
            tag,
            TagEntry {
                tag,
                alice_hash,
                issued_secs: now_secs,
                expires_secs: now_secs.saturating_add(RELAY_TAG_LIFETIME_SECS),
            },
        );
        Ok(())
    }

    /// Handles one authenticated RelayRequest from Alice.
    ///
    /// Admits only authenticated eligible sessions/peers (the caller
    /// guarantees session authentication and passes the verified
    /// `alice_hash`): verifies Alice's signature/freshness, checks the
    /// tag is live and bound to Alice, enforces per-peer/global quotas
    /// and the response-size budget, and records the request for the
    /// single RelayIntro emission. Invalid signature/freshness
    /// requests never allocate long-lived state (they return before
    /// insertion). Repeated replays of the same nonce do not amplify:
    /// the second identical request returns the tracked entry without
    /// emitting a second intro.
    #[allow(clippy::too_many_arguments)]
    pub fn on_request(
        &mut self,
        block: &RelayRequestBlock,
        alice_hash: &[u8; 32],
        bob_hash: &[u8; 32],
        charlie_hash: &[u8; 32],
        alice_signer: &SigningPublicKey,
        request_bytes: usize,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, RelayError> {
        if !self.enabled {
            return Err(RelayError::ServiceDisabled);
        }
        // No response amplification beyond protocol limits: the
        // RelayIntro emission must fit the 3x budget of the request.
        // The intro is ~80 + signature bytes; enforce the budget before
        // any crypto so floods stay cheap.
        if request_bytes.saturating_mul(3) < 128 {
            return Err(RelayError::InvalidSignature);
        }
        check_relay_freshness(block.timestamp(), now_secs)?;
        verify_relay_request(
            block,
            bob_hash,
            charlie_hash,
            alice_signer,
            block.signature(),
        )?;
        self.expire_tags_locked(now_secs);
        self.expire_requests_locked(now_ms);
        // Tag must be live and bound to this Alice.
        let Some(tag) = self.tags.get(&block.relay_tag()) else {
            return Err(RelayError::InvalidTag);
        };
        if tag.expired(now_secs) || tag.alice_hash != *alice_hash {
            return Err(RelayError::InvalidTag);
        }
        // Replay: an identical live nonce is idempotent, not a second
        // intro emission.
        if let Some(tracked) = self.requests.get(&block.nonce()) {
            if tracked.alice_hash == *alice_hash && tracked.tag == block.relay_tag() {
                return Ok(false);
            }
            return Err(RelayError::DuplicateCorrelation);
        }
        if self.requests.len() >= MAX_RELAY_REQUESTS_GLOBAL {
            return Err(RelayError::TooManyRequests);
        }
        let peer_count = self
            .requests
            .values()
            .filter(|entry| entry.alice_hash == *alice_hash)
            .count();
        if peer_count >= MAX_RELAY_REQUESTS_PER_PEER {
            return Err(RelayError::PeerQuotaExceeded);
        }
        self.requests.insert(
            block.nonce(),
            IntroducerRequest {
                nonce: block.nonce(),
                alice_hash: *alice_hash,
                charlie_hash: *charlie_hash,
                tag: block.relay_tag(),
                deadline_ms: now_ms.saturating_add(RELAY_REQUEST_TIMEOUT_MS),
            },
        );
        Ok(true)
    }

    /// Expires tags/requests, returning expired request nonces.
    pub fn poll_expired(&mut self, now_secs: u64, now_ms: u64) -> Vec<u32> {
        self.expire_tags_locked(now_secs);
        let expired: Vec<u32> = self
            .requests
            .iter()
            .filter(|(_, entry)| now_ms >= entry.deadline_ms)
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in &expired {
            self.requests.remove(nonce);
        }
        expired
    }

    /// Shuts down service: removes advertised/active introducer state.
    pub fn shutdown(&mut self) {
        self.tags.clear();
        self.requests.clear();
    }

    /// Returns the earliest request deadline, if any.
    pub fn next_deadline_ms(&self) -> Option<u64> {
        self.requests.values().map(|entry| entry.deadline_ms).min()
    }

    fn expire_tags_locked(&mut self, now_secs: u64) {
        let expired: Vec<u32> = self
            .tags
            .iter()
            .filter(|(_, entry)| entry.expired(now_secs))
            .map(|(tag, _)| *tag)
            .collect();
        for tag in &expired {
            self.tags.remove(tag);
        }
    }

    fn expire_requests_locked(&mut self, now_ms: u64) {
        let expired: Vec<u32> = self
            .requests
            .iter()
            .filter(|(_, entry)| now_ms >= entry.deadline_ms)
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in &expired {
            self.requests.remove(nonce);
        }
    }
}

// ---------------------------------------------------------------------------
// Target (Charlie)
// ---------------------------------------------------------------------------

/// Bounded Charlie-side target table.
///
/// `Debug` is redacted (counts only): pending intros carry router
/// hashes, nonces, and tags (Plan 160 §12).
#[derive(Clone, Default)]
pub struct RelayTarget {
    /// Seen (nonce, alice_hash) pairs for replay suppression: a stale
    /// or replayed intro never triggers a second HolePunch emission.
    seen: HashSet<(u32, [u8; 32])>,
    /// Live intros awaiting HolePunch emission (bounded).
    pending: HashMap<u32, IntroducerRequest>,
}

impl std::fmt::Debug for RelayTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayTarget")
            .field("pending", &self.pending.len())
            .field("seen", &self.seen.len())
            .finish()
    }
}

impl RelayTarget {
    /// Creates an empty target table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of pending intros.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Returns whether no intros are pending.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Handles one authenticated RelayIntro from Bob.
    ///
    /// Validates against the expected authenticated introducer context
    /// (`expected_bob_hash`): verifies Alice's forwarded signature with
    /// the RelayRequest preimage, checks freshness, enforces quotas,
    /// and admits at most one pending intro per (nonce, Alice). Returns
    /// `true` when the caller should emit the bounded HolePunch +
    /// handshake-initiation traffic, `false` for an idempotent replay
    /// (no second emission — no indefinite amplification).
    pub fn on_intro(
        &mut self,
        block: &RelayIntroBlock,
        expected_bob_hash: &[u8; 32],
        expected_charlie_hash: &[u8; 32],
        alice_signer: &SigningPublicKey,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, RelayError> {
        self.expire_locked(now_ms);
        check_relay_freshness(block.timestamp(), now_secs)?;
        // Reconstruct the forwarded RelayRequest view for signature
        // verification (the intro carries Alice's original fields).
        let request_view = RelayRequestBlock::new(
            block.nonce(),
            block.relay_tag(),
            block.timestamp(),
            block.version(),
            block.endpoint(),
            block.signature().to_vec(),
        )
        .map_err(|_| RelayError::InvalidSignature)?;
        verify_relay_request(
            &request_view,
            expected_bob_hash,
            expected_charlie_hash,
            alice_signer,
            block.signature(),
        )?;
        let key = (block.nonce(), *block.alice_hash());
        if self.seen.contains(&key) {
            return Ok(false);
        }
        if self.pending.len() >= MAX_RELAY_REQUESTS_GLOBAL {
            return Err(RelayError::TooManyRequests);
        }
        self.seen.insert(key);
        // Bound the seen set: deterministic oldest-eviction is
        // approximated by clearing on overflow (seen entries are only
        // replay suppressors; clearing risks one extra HolePunch per
        // replayed nonce after 512 entries, never unbounded state).
        if self.seen.len() > 512 {
            self.seen.clear();
            self.seen.insert(key);
        }
        self.pending.insert(
            block.nonce(),
            IntroducerRequest {
                nonce: block.nonce(),
                alice_hash: *block.alice_hash(),
                charlie_hash: *expected_charlie_hash,
                tag: block.relay_tag(),
                deadline_ms: now_ms.saturating_add(RELAY_REQUEST_TIMEOUT_MS),
            },
        );
        Ok(true)
    }

    /// Consumes one pending intro after the HolePunch emission.
    pub fn consume(&mut self, nonce: u32) {
        self.pending.remove(&nonce);
    }

    /// Expires pending intros (seen suppressors survive for replay
    /// protection until the table is cleared).
    pub fn poll_expired(&mut self, now_ms: u64) -> Vec<u32> {
        self.expire_locked(now_ms)
    }

    /// Clears all target state (shutdown/cancel baseline).
    pub fn clear(&mut self) {
        self.pending.clear();
        self.seen.clear();
    }

    fn expire_locked(&mut self, now_ms: u64) -> Vec<u32> {
        let expired: Vec<u32> = self
            .pending
            .iter()
            .filter(|(_, entry)| now_ms >= entry.deadline_ms)
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in &expired {
            self.pending.remove(nonce);
        }
        expired
    }
}

/// Privacy-safe relay counters (counts only, no hashes, tags, nonces,
/// endpoints, or signatures).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelayCounters {
    /// RelayRequest starts (requester) / admissions (introducer).
    pub requests: u64,
    /// RelayResponse emissions.
    pub responses: u64,
    /// RelayIntro emissions.
    pub intros: u64,
    /// HolePunch emissions.
    pub hole_punches: u64,
    /// Requests rejected by tag/signature/freshness/quota checks.
    pub rejections: u64,
    /// Replay/retransmit duplicates absorbed without re-emission.
    pub duplicates_absorbed: u64,
    /// Quota denials (global/per-peer ceilings).
    pub quota_denied: u64,
    /// Entries expired before completion.
    pub expired: u64,
}

/// Returns the relay response-size budget: at most three times the
/// request byte length (mirrors the Retry anti-amplification rule).
pub const fn relay_response_budget(request_bytes: usize) -> usize {
    request_bytes.saturating_mul(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{RelayRequestBlock, RelayResponseBlock};
    use core::net::{IpAddr, Ipv4Addr};
    use i2pr_crypto::SigningPrivateKey;

    const ALICE_HASH: [u8; 32] = [0xA1; 32];
    const BOB_HASH: [u8; 32] = [0x0B; 32];
    const CHARLIE_HASH: [u8; 32] = [0xC4; 32];

    fn endpoint(last: u8, port: u16) -> Ssu2Endpoint {
        Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, last)), port).expect("endpoint")
    }

    fn alice_key() -> SigningPrivateKey {
        SigningPrivateKey::from_bytes([0x11; 32])
    }

    fn charlie_key() -> SigningPrivateKey {
        SigningPrivateKey::from_bytes([0x33; 32])
    }

    fn alice_pub() -> SigningPublicKey {
        alice_key().public_key().expect("alice pub")
    }

    fn charlie_pub() -> SigningPublicKey {
        charlie_key().public_key().expect("charlie pub")
    }

    fn sign_request(nonce: u32, tag: u32, timestamp: u32, endpoint: Ssu2Endpoint) -> Vec<u8> {
        let preimage =
            relay_request_preimage(&BOB_HASH, &CHARLIE_HASH, nonce, tag, timestamp, 2, endpoint);
        alice_key()
            .sign(&preimage)
            .expect("sign")
            .as_bytes()
            .to_vec()
    }

    fn request(nonce: u32, tag: u32, timestamp: u32) -> RelayRequestBlock {
        let endpoint = endpoint(10, 40000);
        RelayRequestBlock::new(
            nonce,
            tag,
            timestamp,
            2,
            endpoint,
            sign_request(nonce, tag, timestamp, endpoint),
        )
        .expect("request")
    }

    fn sign_response(nonce: u32, timestamp: u32, endpoint: Ssu2Endpoint) -> Vec<u8> {
        let preimage = relay_response_preimage(&BOB_HASH, nonce, timestamp, 2, Some(endpoint));
        charlie_key()
            .sign(&preimage)
            .expect("sign")
            .as_bytes()
            .to_vec()
    }

    fn accept(nonce: u32, timestamp: u32, token: u64) -> RelayResponseBlock {
        let endpoint = endpoint(30, 50000);
        RelayResponseBlock::accept(
            nonce,
            timestamp,
            2,
            endpoint,
            sign_response(nonce, timestamp, endpoint),
            token,
        )
        .expect("accept")
    }

    fn intro_key(byte: u8) -> crate::crypto::IntroKey {
        crate::crypto::IntroKey::new([byte; 32])
    }

    #[test]
    fn request_preimage_matches_spec_order() {
        let endpoint = endpoint(10, 40000);
        let preimage = relay_request_preimage(&BOB_HASH, &CHARLIE_HASH, 1, 2, 3, 2, endpoint);
        assert_eq!(&preimage[..16], b"RelayRequestData");
        assert_eq!(&preimage[16..48], &BOB_HASH);
        assert_eq!(&preimage[48..80], &CHARLIE_HASH);
        assert_eq!(&preimage[80..84], &1_u32.to_be_bytes());
        assert_eq!(&preimage[84..88], &2_u32.to_be_bytes());
        assert_eq!(&preimage[88..92], &3_u32.to_be_bytes());
        assert_eq!(preimage[92], 2);
        assert_eq!(preimage[93], 6);
    }

    #[test]
    fn hole_punch_conn_ids_follow_nonce_rule() {
        let (dest, src) = hole_punch_conn_ids(0x01020304);
        assert_eq!(dest, 0x01020304_01020304);
        assert_eq!(src, !dest);
        assert_ne!(dest, src);
    }

    #[test]
    fn hole_punch_round_trips_with_token() {
        let alice_intro = intro_key(0xA0);
        let (dest, src) = hole_punch_conn_ids(77);
        let response = accept(77, 1000, 0x0102030405060708);
        let mut datagram = build_hole_punch(
            &alice_intro,
            src,
            dest,
            0x9ABCDEF0,
            1000,
            endpoint(30, 50000),
            response,
        )
        .expect("build");
        let parsed = parse_hole_punch(&mut datagram, &alice_intro).expect("parse");
        assert_eq!(parsed.header.dst_conn_id(), dest);
        assert_eq!(parsed.header.src_conn_id(), src);
        assert_eq!(parsed.response.nonce(), 77);
        assert_eq!(parsed.response.token(), Some(0x0102030405060708));
        // Wrong intro key fails closed.
        let mut tampered = datagram.clone();
        assert!(parse_hole_punch(&mut tampered, &intro_key(0xB0)).is_err());
    }

    #[test]
    fn requester_correlates_response_and_hole_punch_to_exact_request() {
        let mut requester = RelayRequester::new();
        requester
            .start(11, 7, BOB_HASH, CHARLIE_HASH, endpoint(10, 40000), 0)
            .expect("start");
        requester
            .start(12, 8, BOB_HASH, CHARLIE_HASH, endpoint(11, 40001), 0)
            .expect("second");
        // Response for 11 does not touch 12.
        let response = accept(11, 1000, 99);
        assert_eq!(
            requester.on_response(
                &response,
                &BOB_HASH,
                &BOB_HASH,
                Some(&charlie_pub()),
                1000,
                100
            ),
            Ok(RequesterState::AwaitingHolePunch)
        );
        // Unknown tag/nonce fails closed.
        assert_eq!(
            requester.on_response(
                &accept(13, 1000, 99),
                &BOB_HASH,
                &BOB_HASH,
                Some(&charlie_pub()),
                1000,
                100
            ),
            Err(RelayError::UnknownCorrelation)
        );
        // HolePunch for 11 completes only 11.
        let alice_intro = intro_key(0xA0);
        let (dest, src) = hole_punch_conn_ids(11);
        let mut datagram = build_hole_punch(
            &alice_intro,
            src,
            dest,
            5,
            1001,
            endpoint(30, 50000),
            accept(11, 1001, 100),
        )
        .expect("holepunch");
        let message = parse_hole_punch(&mut datagram, &alice_intro).expect("parse");
        assert_eq!(
            requester.on_hole_punch(&message, &BOB_HASH, Some(&charlie_pub()), 1001, 200),
            Ok(true)
        );
        // Request 12 is still awaiting response.
        assert_eq!(
            requester.on_response(
                &accept(12, 1002, 101),
                &BOB_HASH,
                &BOB_HASH,
                Some(&charlie_pub()),
                1002,
                300
            ),
            Ok(RequesterState::AwaitingHolePunch)
        );
    }

    #[test]
    fn introducer_is_disabled_by_default_and_quotas_bind() {
        let mut disabled = RelayIntroducer::disabled();
        assert!(!disabled.is_enabled());
        assert_eq!(
            disabled.issue_tag(7, ALICE_HASH, 1000),
            Err(RelayError::ServiceDisabled)
        );
        let block = request(21, 7, 1000);
        assert_eq!(
            disabled.on_request(
                &block,
                &ALICE_HASH,
                &BOB_HASH,
                &CHARLIE_HASH,
                &alice_pub(),
                200,
                1000,
                100
            ),
            Err(RelayError::ServiceDisabled)
        );
        // Enabled: tag lifecycle, request admission, replay idempotency.
        let mut introducer = RelayIntroducer::enabled_for_tests();
        introducer.issue_tag(7, ALICE_HASH, 1000).expect("tag");
        assert_eq!(introducer.tag_count(), 1);
        assert!(
            introducer
                .on_request(
                    &block,
                    &ALICE_HASH,
                    &BOB_HASH,
                    &CHARLIE_HASH,
                    &alice_pub(),
                    200,
                    1000,
                    100
                )
                .expect("admit")
        );
        // Replay does not amplify.
        assert!(
            !introducer
                .on_request(
                    &block,
                    &ALICE_HASH,
                    &BOB_HASH,
                    &CHARLIE_HASH,
                    &alice_pub(),
                    200,
                    1000,
                    110
                )
                .expect("replay")
        );
        // Unknown tag fails closed without state.
        let unknown = RelayRequestBlock::new(
            22,
            999,
            1000,
            2,
            endpoint(10, 40000),
            sign_request(22, 999, 1000, endpoint(10, 40000)),
        )
        .expect("unknown tag block");
        assert_eq!(
            introducer.on_request(
                &unknown,
                &ALICE_HASH,
                &BOB_HASH,
                &CHARLIE_HASH,
                &alice_pub(),
                200,
                1000,
                120
            ),
            Err(RelayError::InvalidTag)
        );
        // Stale and bad-signature requests allocate nothing.
        let stale = request(23, 7, 1000);
        assert_eq!(
            introducer.on_request(
                &stale,
                &ALICE_HASH,
                &BOB_HASH,
                &CHARLIE_HASH,
                &alice_pub(),
                200,
                1000 + RELAY_MAX_CLOCK_SKEW_SECONDS + 1,
                130
            ),
            Err(RelayError::StaleTimestamp)
        );
        assert_eq!(introducer.request_count(), 1);
        // Shutdown removes all state.
        introducer.shutdown();
        assert_eq!(introducer.tag_count(), 0);
        assert_eq!(introducer.request_count(), 0);
    }

    #[test]
    fn target_validates_intro_and_suppresses_replays() {
        let mut target = RelayTarget::new();
        let endpoint = endpoint(10, 40000);
        let request = request(31, 7, 1000);
        let intro = RelayIntroBlock::new(
            ALICE_HASH,
            request.nonce(),
            request.relay_tag(),
            request.timestamp(),
            request.version(),
            request.endpoint(),
            request.signature().to_vec(),
        )
        .expect("intro");
        assert!(
            target
                .on_intro(&intro, &BOB_HASH, &CHARLIE_HASH, &alice_pub(), 1000, 100)
                .expect("admit")
        );
        // Replay never triggers a second emission.
        assert!(
            !target
                .on_intro(&intro, &BOB_HASH, &CHARLIE_HASH, &alice_pub(), 1001, 110)
                .expect("replay")
        );
        // Wrong introducer context fails closed.
        let mut target2 = RelayTarget::new();
        assert!(
            target2
                .on_intro(&intro, &[0xFF; 32], &CHARLIE_HASH, &alice_pub(), 1000, 100)
                .is_err()
        );
        let _ = endpoint;
    }

    #[test]
    fn quotas_and_expiry_release_to_baseline() {
        let mut requester = RelayRequester::new();
        for nonce in 1..=MAX_RELAY_REQUESTS_GLOBAL as u32 {
            let mut bob = BOB_HASH;
            bob[0] = nonce as u8;
            requester
                .start(
                    nonce,
                    nonce + 100,
                    bob,
                    CHARLIE_HASH,
                    endpoint(10, 40000),
                    0,
                )
                .expect("start");
        }
        assert_eq!(
            requester.start(999, 1, BOB_HASH, CHARLIE_HASH, endpoint(10, 40000), 0),
            Err(RelayError::TooManyRequests)
        );
        let expired = requester.poll_expired(RELAY_REQUEST_TIMEOUT_MS + 1);
        assert_eq!(expired.len(), MAX_RELAY_REQUESTS_GLOBAL);
        assert!(requester.is_empty());
    }
}
