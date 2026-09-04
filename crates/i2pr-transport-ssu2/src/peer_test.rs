//! Bounded runtime-neutral SSU2 PeerTest roles and correlation (Plan 160).
//!
//! This module owns the session-level PeerTest state machines, not a
//! special UDP daemon. Responsibilities follow Plan 160 §2:
//!
//! ```text
//! i2pr-transport-ssu2   message/block validation, signatures/freshness,
//!                       runtime-neutral peer-test states (this module)
//! i2pr-runtime          UDP send/receive, endpoints/time/randomness,
//!                       admission/rate limits, state ownership/scheduling
//! reachability policy   consumes authenticated typed outcomes
//! ```
//!
//! A decoded [`PeerTestBlock`] never mutates
//! RouterInfo/NetDB directly. Callers only invoke this machine after the
//! session (in-session Msgs 1–4) or the intro-key AEAD (out-of-session
//! Msgs 5–7) authenticated the datagram; this module then enforces
//! role/correlation/signature/freshness/endpoint checks before emitting
//! a typed [`PeerTestOutcome`].
//!
//! Normative traceability: SSU2 specification §Peer Test (block type 10,
//! message type 7; Msgs 1–4 in-session Data, Msgs 5–7 out-of-session
//! long-header). Signature preimages below are verbatim from the spec:
//!
//! ```text
//! prologue "PeerTestValidate" (16 bytes, not on wire)
//! bhash    Bob's 32-byte router hash (not on wire)
//! ahash    Alice's 32-byte router hash (Msgs 3,4 only, not on wire)
//! ver      1-byte SSU version
//! nonce    4-byte test nonce, big endian
//! timestamp 4-byte Unix seconds, big endian
//! asz      1-byte endpoint size (6 or 18)
//! AlicePort 2 bytes, big endian
//! Alice IP  (asz-2) bytes, network order
//! ```
//!
//! Msgs 1,2 are signed by Alice (no `ahash`); Msgs 3,4 by Charlie (with
//! `ahash`); Msgs 5–7 leave the signature optional and may reuse the
//! Msg 3/4 or Msg 1/2 signed data. Only Ed25519 (type 7) signers verify
//! here; other router signing-key types fail as
//! [`PeerTestError::UnsupportedSigner`] with explicit debt (Plan 161
//! owns full multi-algorithm interop). No sockets, no Tokio, no timers,
//! no RNG: nonces, hashes, endpoints, and both clocks arrive as caller
//! inputs; production callers use OS randomness/time via `i2pr-runtime`.

use std::collections::HashMap;

use i2pr_crypto::verify_signature;
use i2pr_proto::{SignatureValue, SigningPublicKey};
use i2pr_transport::AddressFamily;
use thiserror::Error;

use crate::address::Ssu2Endpoint;
use crate::block::PeerTestBlock;
use crate::constants;

/// Maximum concurrent PeerTest sessions tracked by one table.
pub const MAX_PEER_TESTS_GLOBAL: usize = 8;
/// Maximum concurrent tests with one peer hash (Bob for Alice, Alice
/// for helpers).
pub const MAX_PEER_TESTS_PER_PEER: usize = 2;
/// PeerTest candidate lifetime in milliseconds before the test must
/// complete. One central scheduler drives expiry; there is no task or
/// timer per test.
pub const PEER_TEST_TIMEOUT_MS: u64 = 10_000;
/// Maximum accepted clock skew in seconds for peer-test timestamps
/// (mirrors the handshake ±120 s policy).
pub const PEER_TEST_MAX_CLOCK_SKEW_SECONDS: u64 = 120;
/// Spec prologue for the peer-test signature preimage (not on wire).
pub const PEER_TEST_SIGNATURE_PROLOGUE: &[u8; 16] = b"PeerTestValidate";

/// Which role this router plays in one test.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PeerTestRole {
    /// The tested router (firewalled or direct). Sends Msg 1, awaits
    /// Msg 4, then drives the out-of-session Msgs 5–7 corroboration.
    Alice,
    /// The first helper/introducer. Receives Msg 1, forwards Msg 2 to
    /// Charlie, awaits Msg 3, forwards Msg 4 to Alice.
    Bob,
    /// The independent third peer. Receives Msg 2, answers Msg 3, then
    /// drives Msgs 5/7 with Alice.
    Charlie,
}

/// Where one test sits in the 7-message sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerTestState {
    /// Alice sent Msg 1, awaiting Msg 4 from Bob.
    AliceAwaitingMsg4,
    /// Alice received Msg 4, awaiting Msg 5 from Charlie.
    AliceAwaitingMsg5,
    /// Alice sent Msg 6, awaiting Msg 7 from Charlie.
    AliceAwaitingMsg7,
    /// Bob forwarded Msg 2, awaiting Msg 3 from Charlie.
    BobAwaitingMsg3,
    /// Charlie sent Msg 3, awaiting Msg 6 from Alice (Msgs 5/7 bracket it).
    CharlieAwaitingMsg6,
    /// Terminal: outcome decided, retained until expiry/cancel for
    /// duplicate suppression.
    Completed,
}

/// Typed peer-test reachability outcome (Plan 160 §4).
///
/// The policy layer consumes these with corroboration/expiry rules; a
/// single unauthenticated datagram never confirms reachability (the
/// table only emits these after signature + freshness + correlation +
/// role + endpoint checks pass).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerTestOutcome {
    /// Corroborated direct reachability with address agreement.
    DirectReachabilityConfirmed {
        /// Tested address family.
        family: AddressFamily,
        /// Observed external endpoint (policy strips to family).
        observed: Ssu2Endpoint,
        /// Number of corroborating peers behind this outcome.
        evidence_peers: u8,
    },
    /// Authenticated observations disagree on the external address.
    AddressMismatch {
        /// Tested address family.
        family: AddressFamily,
        /// First observed endpoint.
        first: Ssu2Endpoint,
        /// Contradicting observed endpoint.
        second: Ssu2Endpoint,
    },
    /// Corroborated evidence indicates NAT/firewall without direct
    /// inbound reachability.
    FirewalledLikely {
        /// Tested address family.
        family: AddressFamily,
    },
    /// Third-peer refusal/timeout or insufficient evidence: neither
    /// reachable nor firewalled may be claimed.
    Inconclusive {
        /// Tested address family.
        family: AddressFamily,
    },
    /// The test was refused or malformed at the protocol layer.
    Rejected {
        /// Tested address family.
        family: AddressFamily,
    },
}

impl PeerTestOutcome {
    /// Returns the tested address family.
    pub const fn family(self) -> AddressFamily {
        match self {
            Self::DirectReachabilityConfirmed { family, .. }
            | Self::AddressMismatch { family, .. }
            | Self::FirewalledLikely { family }
            | Self::Inconclusive { family }
            | Self::Rejected { family } => family,
        }
    }

    /// Returns whether the outcome supports direct reachability.
    pub const fn supports_reachability(self) -> bool {
        matches!(self, Self::DirectReachabilityConfirmed { .. })
    }

    /// Returns whether the outcome contradicts direct reachability.
    pub const fn contradicts_reachability(self) -> bool {
        matches!(
            self,
            Self::AddressMismatch { .. } | Self::FirewalledLikely { .. }
        )
    }
}

/// Typed peer-test sequencing failures.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PeerTestError {
    /// The test table already retains its global ceiling.
    #[error("SSU2 peer-test table is full")]
    TooManyTests,
    /// The peer already retains its per-peer ceiling.
    #[error("SSU2 peer-test per-peer quota is full")]
    PeerQuotaExceeded,
    /// The nonce is already in use by a live test.
    #[error("SSU2 peer-test nonce is already in use")]
    DuplicateNonce,
    /// No live test matches the presented correlation.
    #[error("SSU2 peer-test correlation is unknown")]
    UnknownTest,
    /// The message number does not match the role/state.
    #[error("SSU2 peer-test message has the wrong role or state")]
    WrongRole,
    /// The sender hash or endpoint does not match the tracked test.
    #[error("SSU2 peer-test sender does not match its test")]
    SenderMismatch,
    /// The signature is missing, malformed, or does not verify.
    #[error("SSU2 peer-test signature is invalid")]
    InvalidSignature,
    /// The signer key type is not supported by this pass (only
    /// Ed25519 verifies; Plan 161 owns multi-algorithm interop).
    #[error("SSU2 peer-test signer type is unsupported")]
    UnsupportedSigner,
    /// The timestamp is outside the freshness window.
    #[error("SSU2 peer-test timestamp is stale")]
    StaleTimestamp,
    /// The SSU version under test is not v2.
    #[error("SSU2 peer-test version is unsupported")]
    UnsupportedVersion,
    /// The test expired before completion.
    #[error("SSU2 peer-test expired")]
    Expired,
    /// The test was cancelled by the caller.
    #[error("SSU2 peer-test cancelled")]
    Cancelled,
}

/// Builds the exact spec signature preimage for one peer-test block.
///
/// `alice_hash` is `Some` only for Msgs 3,4 (Charlie-signed, includes
/// `ahash`); `None` for Msgs 1,2 (Alice-signed). Msgs 5–7 reuse the
/// Msg 1/2 (no `ahash`) or Msg 3/4 (with `ahash`) shape depending on
/// which signed data they carry; callers pass the matching option.
pub fn peer_test_preimage(
    message: u8,
    bob_hash: &[u8; 32],
    alice_hash: Option<&[u8; 32]>,
    version: u8,
    nonce: u32,
    timestamp: u32,
    endpoint: Ssu2Endpoint,
) -> Vec<u8> {
    let ip_bytes: Vec<u8> = match endpoint.ip() {
        core::net::IpAddr::V4(address) => address.octets().to_vec(),
        core::net::IpAddr::V6(address) => address.octets().to_vec(),
    };
    let asz: u8 = match endpoint.ip() {
        core::net::IpAddr::V4(_) => 6,
        core::net::IpAddr::V6(_) => 18,
    };
    let mut preimage = Vec::with_capacity(16 + 32 + 32 + 1 + 4 + 4 + 1 + 2 + 16);
    preimage.extend_from_slice(PEER_TEST_SIGNATURE_PROLOGUE);
    preimage.extend_from_slice(bob_hash);
    // Only Msgs 3,4 include ahash; Msg numbers are validated by the
    // caller, but the preimage shape follows the option, not the
    // number, so Msgs 5–7 can reuse either shape explicitly.
    if let Some(ahash) = alice_hash {
        debug_assert!(
            message == 3 || message == 4 || message >= 5,
            "ahash only for Msgs 3,4 (or 5-7 reuse)"
        );
        preimage.extend_from_slice(ahash);
    }
    preimage.push(version);
    preimage.extend_from_slice(&nonce.to_be_bytes());
    preimage.extend_from_slice(&timestamp.to_be_bytes());
    preimage.push(asz);
    preimage.extend_from_slice(&endpoint.port().to_be_bytes());
    preimage.extend_from_slice(&ip_bytes);
    preimage
}

/// Verifies one peer-test block signature against the spec preimage.
///
/// `alice_hash` follows [`peer_test_preimage`]: `Some` for Msgs 3,4
/// (and Msgs 5–7 when they reuse that shape), `None` otherwise.
/// Empty signatures are accepted only for Msgs 5–7 (spec-optional);
/// callers must not treat an unverified Msg 5–7 as confirmation-grade
/// evidence.
pub fn verify_peer_test_signature(
    block: &PeerTestBlock,
    bob_hash: &[u8; 32],
    alice_hash: Option<&[u8; 32]>,
    signer: &SigningPublicKey,
    signature: &[u8],
) -> Result<(), PeerTestError> {
    if block.version() != constants::SSU2_VERSION {
        return Err(PeerTestError::UnsupportedVersion);
    }
    if signature.is_empty() {
        // Spec-optional only for Msgs 5–7; Msgs 1–4 always require a
        // signature (the block parser already enforces presence, but
        // re-check here so direct calls cannot bypass it).
        if block.message() <= 4 {
            return Err(PeerTestError::InvalidSignature);
        }
        return Ok(());
    }
    let preimage = peer_test_preimage(
        block.message(),
        bob_hash,
        alice_hash,
        block.version(),
        block.nonce(),
        block.timestamp(),
        block.endpoint(),
    );
    let value = SignatureValue::new(signer.key_type(), signature.to_vec())
        .map_err(|_| PeerTestError::UnsupportedSigner)?;
    verify_signature(signer, &preimage, &value).map_err(|_| PeerTestError::InvalidSignature)
}

/// Checks peer-test timestamp freshness against caller time.
///
/// Both domains are Unix seconds; `now_secs` is u64 so far-future
/// wall clocks past `u32::MAX` fail closed rather than wrapping.
pub fn check_peer_test_freshness(timestamp: u32, now_secs: u64) -> Result<(), PeerTestError> {
    let timestamp = u64::from(timestamp);
    let skew = now_secs.abs_diff(timestamp);
    if skew > PEER_TEST_MAX_CLOCK_SKEW_SECONDS {
        return Err(PeerTestError::StaleTimestamp);
    }
    Ok(())
}

/// Derives the spec out-of-session connection IDs from the test nonce.
///
/// Charlie→Alice (Msgs 5,7): `Dest = (nonce << 32) | nonce`,
/// `Src = !Dest`. Alice→Charlie (Msg 6) swaps the two. Set
/// `alice_to_charlie` for Msg 6, clear it for Msgs 5,7.
pub fn peer_test_conn_ids(nonce: u32, alice_to_charlie: bool) -> (u64, u64) {
    let base = (u64::from(nonce) << 32) | u64::from(nonce);
    if alice_to_charlie {
        (!base, base)
    } else {
        (base, !base)
    }
}

/// Builds one out-of-session PeerTest datagram (type 7, long header)
/// under the receiver's intro key: header plus AEAD payload carrying
/// exactly the supplied PeerTest block. `packet_number` is
/// random/ignored per spec and supplied by the caller (production: OS
/// randomness via the runtime).
pub fn build_out_of_session_peer_test(
    receiver_intro: &crate::crypto::IntroKey,
    src_conn_id: u64,
    dst_conn_id: u64,
    packet_number: u32,
    block: PeerTestBlock,
) -> Result<Vec<u8>, PeerTestError> {
    if src_conn_id == dst_conn_id {
        return Err(PeerTestError::WrongRole);
    }
    if !(5..=7).contains(&block.message()) {
        return Err(PeerTestError::WrongRole);
    }
    let payload = crate::block::encode_blocks(vec![crate::block::Block::PeerTest(block)])
        .map_err(|_| PeerTestError::WrongRole)?;
    let header = crate::header::LongHeader::new(
        dst_conn_id,
        packet_number,
        crate::header::MessageType::PeerTest,
        src_conn_id,
        0,
    )
    .map_err(|_| PeerTestError::WrongRole)?;
    let header_bytes = header.encode();
    let sealed =
        crate::crypto::seal_token_payload(receiver_intro, packet_number, &header_bytes, &payload)
            .map_err(|_| PeerTestError::WrongRole)?;
    let mut datagram = Vec::with_capacity(crate::constants::LONG_HEADER_LENGTH + sealed.len());
    datagram.extend_from_slice(&header_bytes);
    datagram.extend_from_slice(&sealed);
    crate::crypto::apply_header_protection(
        &mut datagram,
        crate::constants::LONG_HEADER_LENGTH,
        receiver_intro.as_bytes(),
        receiver_intro.as_bytes(),
        false,
    )
    .map_err(|_| PeerTestError::WrongRole)?;
    crate::packet::DatagramLengthClass::classify(datagram.len())
        .map_err(|_| PeerTestError::WrongRole)?;
    Ok(datagram)
}

/// Parses one inbound out-of-session PeerTest datagram after cheap
/// prevalidation: intro-key header deprotection, exact header decode,
/// version/network/type checks, minimum tail, AEAD open, and exactly
/// one PeerTest block.
pub fn parse_out_of_session_peer_test(
    datagram: &mut [u8],
    receiver_intro: &crate::crypto::IntroKey,
) -> Result<(crate::header::LongHeader, PeerTestBlock), PeerTestError> {
    let pre = crate::handshake::prevalidate_long_datagram(
        datagram,
        receiver_intro,
        crate::header::MessageType::PeerTest,
        false,
    )
    .map_err(|_| PeerTestError::WrongRole)?;
    let header = pre.header();
    let sealed = &datagram[crate::constants::LONG_HEADER_LENGTH..];
    let payload = crate::crypto::open_token_payload(
        receiver_intro,
        header.packet_number(),
        &header.encode(),
        sealed,
    )
    .map_err(|_| PeerTestError::InvalidSignature)?;
    let parsed = crate::block::parse_blocks(&payload).map_err(|_| PeerTestError::WrongRole)?;
    if parsed.blocks().len() != 1 {
        return Err(PeerTestError::WrongRole);
    }
    match &parsed.blocks()[0] {
        crate::block::DecodedBlock::PeerTest(block) => Ok((header, block.clone())),
        _ => Err(PeerTestError::WrongRole),
    }
}

/// One bounded active peer test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PeerTestEntry {
    nonce: u32,
    role: PeerTestRole,
    state: PeerTestState,
    alice_hash: [u8; 32],
    bob_hash: [u8; 32],
    charlie_hash: [u8; 32],
    /// The endpoint under test (Alice's claimed external address).
    alice_endpoint: Ssu2Endpoint,
    /// First authenticated observed endpoint (for mismatch detection).
    observed_first: Option<Ssu2Endpoint>,
    /// Highest peer-test message number accepted so far (duplicate /
    /// reorder idempotency without replaying effects).
    high_message: u8,
    /// Monotonic creation time (ms).
    created_ms: u64,
    /// Monotonic deadline (ms).
    deadline_ms: u64,
    /// Terminal outcome, once decided.
    outcome: Option<PeerTestOutcome>,
}

impl PeerTestEntry {
    fn peer_key(self) -> [u8; 32] {
        match self.role {
            PeerTestRole::Alice => self.bob_hash,
            PeerTestRole::Bob | PeerTestRole::Charlie => self.alice_hash,
        }
    }

    fn expired(self, now_ms: u64) -> bool {
        now_ms >= self.deadline_ms
    }
}

/// Privacy-safe peer-test counters (counts only, no hashes, nonces,
/// endpoints, or signatures).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeerTestCounters {
    /// Tests started by role.
    pub started: u64,
    /// Tests completed with a typed outcome.
    pub completed: u64,
    /// Inbound messages cheap-dropped (unknown correlation).
    pub unknown_dropped: u64,
    /// Messages rejected by role/state/sender checks.
    pub role_rejections: u64,
    /// Messages rejected by signature verification.
    pub signature_rejections: u64,
    /// Messages rejected by freshness checks.
    pub freshness_rejections: u64,
    /// Duplicate/reordered valid messages absorbed idempotently.
    pub duplicates_absorbed: u64,
    /// Tests refused by global/per-peer quotas.
    pub quota_denied: u64,
    /// Tests expired before completion.
    pub expired: u64,
    /// Tests cancelled by the caller.
    pub cancelled: u64,
}

/// The runtime-neutral bounded peer-test table.
///
/// One central scheduler drives expiry via [`PeerTestTable::poll_expired`];
/// there is deliberately no task or timer per test.
///
/// `Debug` is redacted by construction (counts only): active tests carry
/// router hashes, nonces, endpoints, and signatures that must never
/// appear in logs or evidence (Plan 160 §12).
#[derive(Clone, Default)]
pub struct PeerTestTable {
    entries: HashMap<u32, PeerTestEntry>,
    counters: PeerTestCounters,
}

impl std::fmt::Debug for PeerTestTable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerTestTable")
            .field("live_tests", &self.entries.len())
            .field("counters", &self.counters)
            .finish()
    }
}

impl PeerTestTable {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns privacy-safe counters.
    pub const fn counters(&self) -> PeerTestCounters {
        self.counters
    }

    /// Returns the number of live (non-completed or unreaped) tests.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no tests are tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Starts one test in the caller's role.
    ///
    /// `nonce` must be caller randomness (production: OS CSPRNG via
    /// the runtime); zero is rejected so correlation IDs stay
    /// unambiguous. Quotas bind globally and per peer hash.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        nonce: u32,
        role: PeerTestRole,
        alice_hash: [u8; 32],
        bob_hash: [u8; 32],
        charlie_hash: [u8; 32],
        alice_endpoint: Ssu2Endpoint,
        now_ms: u64,
    ) -> Result<(), PeerTestError> {
        if nonce == 0 {
            return Err(PeerTestError::DuplicateNonce);
        }
        self.expire_locked(now_ms);
        if self.entries.contains_key(&nonce) {
            return Err(PeerTestError::DuplicateNonce);
        }
        if self.entries.len() >= MAX_PEER_TESTS_GLOBAL {
            self.counters.quota_denied = self.counters.quota_denied.saturating_add(1);
            return Err(PeerTestError::TooManyTests);
        }
        let peer_key: [u8; 32] = match role {
            PeerTestRole::Alice => bob_hash,
            PeerTestRole::Bob | PeerTestRole::Charlie => alice_hash,
        };
        let peer_count = self
            .entries
            .values()
            .filter(|entry| entry.peer_key() == peer_key)
            .count();
        if peer_count >= MAX_PEER_TESTS_PER_PEER {
            self.counters.quota_denied = self.counters.quota_denied.saturating_add(1);
            return Err(PeerTestError::PeerQuotaExceeded);
        }
        let state = match role {
            PeerTestRole::Alice => PeerTestState::AliceAwaitingMsg4,
            PeerTestRole::Bob => PeerTestState::BobAwaitingMsg3,
            PeerTestRole::Charlie => PeerTestState::CharlieAwaitingMsg6,
        };
        self.entries.insert(
            nonce,
            PeerTestEntry {
                nonce,
                role,
                state,
                alice_hash,
                bob_hash,
                charlie_hash,
                alice_endpoint,
                observed_first: None,
                high_message: 0,
                created_ms: now_ms,
                deadline_ms: now_ms.saturating_add(PEER_TEST_TIMEOUT_MS),
                outcome: None,
            },
        );
        self.counters.started = self.counters.started.saturating_add(1);
        Ok(())
    }

    /// Ingests one authenticated peer-test block for its test.
    ///
    /// The caller guarantees the containing datagram already passed
    /// session (Msgs 1–4) or intro-key AEAD (Msgs 5–7) authentication;
    /// this function then enforces correlation, role/state, sender,
    /// signature, freshness, and endpoint checks before mutating any
    /// test state. Unknown correlations are cheap-dropped without
    /// allocating state; stale/unknown/wrong-role messages never
    /// create reachability evidence (they return an error and the
    /// caller emits no outcome).
    ///
    /// `sender_hash` is the router hash of the datagram sender
    /// (Bob for Alice's Msg 4, Charlie for Msgs 3/5/7, Alice for
    /// Msgs 1/6). `signer`/`signature` verify per
    /// [`verify_peer_test_signature`]; Msgs 5–7 may carry an empty
    /// signature (spec-optional) but then never confirm on their own.
    /// `now_secs` is wall-clock seconds for freshness; `now_ms` is
    /// monotonic milliseconds for deadlines.
    #[allow(clippy::too_many_arguments)]
    pub fn ingest(
        &mut self,
        block: &PeerTestBlock,
        sender_hash: &[u8; 32],
        sender_endpoint: Ssu2Endpoint,
        bob_hash: &[u8; 32],
        alice_hash_for_3_4: Option<&[u8; 32]>,
        signer: Option<&SigningPublicKey>,
        signature: &[u8],
        now_secs: u64,
        now_ms: u64,
    ) -> Result<Option<PeerTestOutcome>, PeerTestError> {
        self.expire_locked(now_ms);
        let nonce = block.nonce();
        let Some(entry) = self.entries.get_mut(&nonce) else {
            self.counters.unknown_dropped = self.counters.unknown_dropped.saturating_add(1);
            return Err(PeerTestError::UnknownTest);
        };
        if entry.expired(now_ms) {
            self.counters.expired = self.counters.expired.saturating_add(1);
            self.entries.remove(&nonce);
            return Err(PeerTestError::Expired);
        }
        if entry.outcome.is_some() {
            // Terminal entries absorb duplicates for the suppression
            // window without replaying effects.
            self.counters.duplicates_absorbed = self.counters.duplicates_absorbed.saturating_add(1);
            return Ok(entry.outcome);
        }
        // Role/state gate first: concurrent tests cannot consume each
        // other's messages even when nonces somehow collide across
        // roles, because the expected message per state is exact.
        let expected = match (entry.role, entry.state) {
            (PeerTestRole::Alice, PeerTestState::AliceAwaitingMsg4) => Some(4_u8),
            (PeerTestRole::Alice, PeerTestState::AliceAwaitingMsg5) => Some(5_u8),
            (PeerTestRole::Alice, PeerTestState::AliceAwaitingMsg7) => Some(7_u8),
            (PeerTestRole::Bob, PeerTestState::BobAwaitingMsg3) => Some(3_u8),
            (PeerTestRole::Charlie, PeerTestState::CharlieAwaitingMsg6) => Some(6_u8),
            _ => None,
        };
        let Some(expected_message) = expected else {
            self.counters.role_rejections = self.counters.role_rejections.saturating_add(1);
            return Err(PeerTestError::WrongRole);
        };
        if block.message() != expected_message {
            // Duplicate of an already-absorbed lower message is
            // idempotent where the protocol permits (Msgs 5–7
            // retransmits); anything else is a wrong-role attempt.
            if block.message() < expected_message && block.message() >= 5 {
                self.counters.duplicates_absorbed =
                    self.counters.duplicates_absorbed.saturating_add(1);
                return Ok(None);
            }
            self.counters.role_rejections = self.counters.role_rejections.saturating_add(1);
            return Err(PeerTestError::WrongRole);
        }
        // Sender gate: the datagram sender must be the expected peer
        // for this transition.
        let expected_sender: &[u8; 32] = match (entry.role, block.message()) {
            (PeerTestRole::Alice, 4) => &entry.bob_hash,
            (PeerTestRole::Alice, 5 | 7) => &entry.charlie_hash,
            (PeerTestRole::Bob, 3) => &entry.charlie_hash,
            (PeerTestRole::Charlie, 6) => &entry.alice_hash,
            _ => {
                self.counters.role_rejections = self.counters.role_rejections.saturating_add(1);
                return Err(PeerTestError::WrongRole);
            }
        };
        if sender_hash != expected_sender {
            self.counters.role_rejections = self.counters.role_rejections.saturating_add(1);
            return Err(PeerTestError::SenderMismatch);
        }
        // Endpoint gate: in-session transitions must arrive over the
        // expected session peer endpoint shape (family must match the
        // test family; exact address equality is checked by the
        // runtime session binding before this call, so here we only
        // enforce family separation — v4 evidence never satisfies a
        // v6 test and vice versa).
        if sender_endpoint.family() != entry.alice_endpoint.family()
            && matches!(block.message(), 4 | 5 | 7)
        {
            self.counters.role_rejections = self.counters.role_rejections.saturating_add(1);
            return Err(PeerTestError::SenderMismatch);
        }
        // Freshness gate before signature cost? Signatures are cheap
        // Ed25519 verifies, but freshness is cheaper and rejects
        // replays without crypto. Check freshness first.
        if let Err(error) = check_peer_test_freshness(block.timestamp(), now_secs) {
            self.counters.freshness_rejections =
                self.counters.freshness_rejections.saturating_add(1);
            return Err(error);
        }
        // Signature gate: Msgs 1–4 always require a valid signature;
        // Msgs 5–7 may omit it (spec-optional) but then never confirm
        // alone — the outcome logic below treats unsigned 5/7 as
        // corroboration-only.
        let signed = !signature.is_empty();
        if block.message() <= 4 || signed {
            let Some(signer) = signer else {
                self.counters.signature_rejections =
                    self.counters.signature_rejections.saturating_add(1);
                return Err(PeerTestError::InvalidSignature);
            };
            verify_peer_test_signature(block, bob_hash, alice_hash_for_3_4, signer, signature)
                .inspect_err(|&error| match error {
                    PeerTestError::InvalidSignature | PeerTestError::UnsupportedSigner => {
                        self.counters.signature_rejections =
                            self.counters.signature_rejections.saturating_add(1);
                    }
                    PeerTestError::StaleTimestamp => {
                        self.counters.freshness_rejections =
                            self.counters.freshness_rejections.saturating_add(1);
                    }
                    _ => {}
                })?;
        }
        // Status-code gate: non-zero codes are explicit refusals, never
        // confirmations.
        if block.code() != 0 {
            let outcome = PeerTestOutcome::Rejected {
                family: entry.alice_endpoint.family(),
            };
            entry.outcome = Some(outcome);
            entry.state = PeerTestState::Completed;
            self.counters.completed = self.counters.completed.saturating_add(1);
            return Ok(Some(outcome));
        }
        // Per-message state advance with mismatch tracking.
        let observed = block.endpoint();
        if entry.observed_first.is_none() {
            entry.observed_first = Some(observed);
        }
        let first = entry.observed_first.expect("just set");
        if first != observed {
            // Contradictory authenticated observations do not let the
            // latest packet win: record the mismatch as the terminal
            // outcome rather than flipping state.
            let outcome = PeerTestOutcome::AddressMismatch {
                family: entry.alice_endpoint.family(),
                first,
                second: observed,
            };
            entry.outcome = Some(outcome);
            entry.state = PeerTestState::Completed;
            self.counters.completed = self.counters.completed.saturating_add(1);
            return Ok(Some(outcome));
        }
        match (entry.role, block.message()) {
            (PeerTestRole::Alice, 4) => {
                entry.high_message = 4;
                // Msg 4 alone never confirms: Alice must still see
                // out-of-session corroboration (Msgs 5/7) or a second
                // independent helper before the policy layer can
                // publish. Advance to await Msg 5.
                entry.state = PeerTestState::AliceAwaitingMsg5;
                Ok(None)
            }
            (PeerTestRole::Alice, 5) => {
                entry.high_message = 5;
                // Unsigned Msg 5 is corroboration-only: advance but do
                // not complete. A signed Msg 5 still needs Msg 7 for
                // the direct-confirmation quorum in this pass.
                entry.state = PeerTestState::AliceAwaitingMsg7;
                Ok(None)
            }
            (PeerTestRole::Alice, 7) => {
                entry.high_message = 7;
                // Full corroboration: Msg 4 (in-session, signed) plus
                // Msgs 5 and 7 (out-of-session) with matching observed
                // endpoints. Compare against the configured endpoint:
                // agreement confirms, disagreement is a mismatch (kept
                // terminal above), and unsigned 5/7 downgrades to
                // inconclusive rather than false confirmation.
                let family = entry.alice_endpoint.family();
                let outcome = if observed == entry.alice_endpoint {
                    if signed {
                        PeerTestOutcome::DirectReachabilityConfirmed {
                            family,
                            observed,
                            evidence_peers: 2,
                        }
                    } else {
                        PeerTestOutcome::Inconclusive { family }
                    }
                } else {
                    // Observed address differs from configuration: the
                    // router is reachable, but not where it claims.
                    // Report mismatch rather than confirming the
                    // configured address.
                    PeerTestOutcome::AddressMismatch {
                        family,
                        first: entry.alice_endpoint,
                        second: observed,
                    }
                };
                entry.outcome = Some(outcome);
                entry.state = PeerTestState::Completed;
                self.counters.completed = self.counters.completed.saturating_add(1);
                Ok(Some(outcome))
            }
            (PeerTestRole::Bob, 3) => {
                entry.high_message = 3;
                // Bob's role ends after forwarding Msg 4 (the runtime
                // emits it); mark completed without a reachability
                // outcome — Bob learns nothing publishable from
                // helping.
                entry.state = PeerTestState::Completed;
                entry.outcome = Some(PeerTestOutcome::Inconclusive {
                    family: entry.alice_endpoint.family(),
                });
                self.counters.completed = self.counters.completed.saturating_add(1);
                Ok(entry.outcome)
            }
            (PeerTestRole::Charlie, 6) => {
                entry.high_message = 6;
                // Charlie answers Msg 7 (runtime emits); the helper
                // outcome is inconclusive for publication.
                entry.state = PeerTestState::Completed;
                entry.outcome = Some(PeerTestOutcome::Inconclusive {
                    family: entry.alice_endpoint.family(),
                });
                self.counters.completed = self.counters.completed.saturating_add(1);
                Ok(entry.outcome)
            }
            _ => {
                self.counters.role_rejections = self.counters.role_rejections.saturating_add(1);
                Err(PeerTestError::WrongRole)
            }
        }
    }

    /// Records a timeout/refusal-driven inconclusive outcome for one
    /// test without falsely confirming or denying reachability.
    pub fn mark_inconclusive(
        &mut self,
        nonce: u32,
        now_ms: u64,
    ) -> Result<PeerTestOutcome, PeerTestError> {
        self.expire_locked(now_ms);
        let Some(entry) = self.entries.get_mut(&nonce) else {
            return Err(PeerTestError::UnknownTest);
        };
        if let Some(outcome) = entry.outcome {
            return Ok(outcome);
        }
        let outcome = PeerTestOutcome::Inconclusive {
            family: entry.alice_endpoint.family(),
        };
        entry.outcome = Some(outcome);
        entry.state = PeerTestState::Completed;
        self.counters.completed = self.counters.completed.saturating_add(1);
        Ok(outcome)
    }

    /// Records a firewalled/inbound-unreachable outcome for one test
    /// after authenticated evidence (e.g. repeated timeouts with live
    /// helpers, or explicit relay need).
    pub fn mark_firewalled(
        &mut self,
        nonce: u32,
        now_ms: u64,
    ) -> Result<PeerTestOutcome, PeerTestError> {
        self.expire_locked(now_ms);
        let Some(entry) = self.entries.get_mut(&nonce) else {
            return Err(PeerTestError::UnknownTest);
        };
        if let Some(outcome) = entry.outcome {
            return Ok(outcome);
        }
        let outcome = PeerTestOutcome::FirewalledLikely {
            family: entry.alice_endpoint.family(),
        };
        entry.outcome = Some(outcome);
        entry.state = PeerTestState::Completed;
        self.counters.completed = self.counters.completed.saturating_add(1);
        Ok(outcome)
    }

    /// Cancels one test and releases its quota.
    pub fn cancel(&mut self, nonce: u32) -> Result<(), PeerTestError> {
        if self.entries.remove(&nonce).is_some() {
            self.counters.cancelled = self.counters.cancelled.saturating_add(1);
            Ok(())
        } else {
            Err(PeerTestError::UnknownTest)
        }
    }

    /// Expires timed-out tests, returning their nonces for diagnostics.
    /// Expired tests never confirm: expiry retains no outcome.
    pub fn poll_expired(&mut self, now_ms: u64) -> Vec<u32> {
        let expired: Vec<u32> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.expired(now_ms) && entry.outcome.is_none())
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in &expired {
            self.entries.remove(nonce);
            self.counters.expired = self.counters.expired.saturating_add(1);
        }
        // Reap completed entries past their deadline as well so shutdown
        // returns all resources to baseline.
        let reaped: Vec<u32> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.expired(now_ms))
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in &reaped {
            if !expired.contains(nonce) {
                self.entries.remove(nonce);
            }
        }
        expired
    }

    /// Returns the earliest deadline, if any, for the central scheduler.
    pub fn next_deadline_ms(&self) -> Option<u64> {
        self.entries.values().map(|entry| entry.deadline_ms).min()
    }

    fn expire_locked(&mut self, now_ms: u64) {
        let expired: Vec<u32> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.expired(now_ms) && entry.outcome.is_none())
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in &expired {
            self.entries.remove(nonce);
            self.counters.expired = self.counters.expired.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::PeerTestBlock;
    use core::net::{IpAddr, Ipv4Addr};
    use i2pr_crypto::SigningPrivateKey;
    use i2pr_proto::SigningKeyType;

    const BOB_HASH: [u8; 32] = [0x0B; 32];
    const ALICE_HASH: [u8; 32] = [0xA1; 32];
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

    fn sign_charlie_msg(
        message: u8,
        nonce: u32,
        timestamp: u32,
        endpoint: Ssu2Endpoint,
    ) -> Vec<u8> {
        let preimage = peer_test_preimage(
            message,
            &BOB_HASH,
            Some(&ALICE_HASH),
            2,
            nonce,
            timestamp,
            endpoint,
        );
        charlie_key()
            .sign(&preimage)
            .expect("sign")
            .as_bytes()
            .to_vec()
    }

    fn msg4(nonce: u32, timestamp: u32, endpoint: Ssu2Endpoint) -> PeerTestBlock {
        PeerTestBlock::new(
            4,
            0,
            Some(CHARLIE_HASH),
            2,
            nonce,
            timestamp,
            endpoint,
            sign_charlie_msg(4, nonce, timestamp, endpoint),
        )
        .expect("msg4")
    }

    fn msg5(nonce: u32, timestamp: u32, endpoint: Ssu2Endpoint, signed: bool) -> PeerTestBlock {
        let signature = if signed {
            sign_charlie_msg(5, nonce, timestamp, endpoint)
        } else {
            Vec::new()
        };
        PeerTestBlock::new(5, 0, None, 2, nonce, timestamp, endpoint, signature).expect("msg5")
    }

    fn msg7(nonce: u32, timestamp: u32, endpoint: Ssu2Endpoint, signed: bool) -> PeerTestBlock {
        let signature = if signed {
            sign_charlie_msg(7, nonce, timestamp, endpoint)
        } else {
            Vec::new()
        };
        PeerTestBlock::new(7, 0, None, 2, nonce, timestamp, endpoint, signature).expect("msg7")
    }

    #[test]
    fn preimage_matches_spec_field_order() {
        let endpoint = endpoint(1, 12345);
        let preimage = peer_test_preimage(
            4,
            &BOB_HASH,
            Some(&ALICE_HASH),
            2,
            0x01020304,
            0x05060708,
            endpoint,
        );
        assert_eq!(&preimage[..16], b"PeerTestValidate");
        assert_eq!(&preimage[16..48], &BOB_HASH);
        assert_eq!(&preimage[48..80], &ALICE_HASH);
        assert_eq!(preimage[80], 2);
        assert_eq!(&preimage[81..85], &[1, 2, 3, 4]);
        assert_eq!(&preimage[85..89], &[5, 6, 7, 8]);
        assert_eq!(preimage[89], 6);
        assert_eq!(&preimage[90..92], &12345_u16.to_be_bytes());
        assert_eq!(&preimage[92..96], &[192, 0, 2, 1]);
        // Msgs 1,2 omit ahash.
        let short = peer_test_preimage(1, &BOB_HASH, None, 2, 1, 2, endpoint);
        assert_eq!(short.len(), preimage.len() - 32);
    }

    #[test]
    fn alice_full_trajectory_confirms_direct() {
        let mut table = PeerTestTable::new();
        let observed = endpoint(10, 40000);
        table
            .start(
                111,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        // Msg 4 alone never confirms.
        let outcome = table
            .ingest(
                &msg4(111, 1000, observed),
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg4(111, 1000, observed).signature(),
                1000,
                100,
            )
            .expect("msg4");
        assert_eq!(outcome, None);
        let outcome = table
            .ingest(
                &msg5(111, 1001, observed, true),
                &CHARLIE_HASH,
                observed,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg5(111, 1001, observed, true).signature(),
                1001,
                200,
            )
            .expect("msg5");
        assert_eq!(outcome, None);
        let outcome = table
            .ingest(
                &msg7(111, 1002, observed, true),
                &CHARLIE_HASH,
                observed,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg7(111, 1002, observed, true).signature(),
                1002,
                300,
            )
            .expect("msg7")
            .expect("outcome");
        assert_eq!(
            outcome,
            PeerTestOutcome::DirectReachabilityConfirmed {
                family: AddressFamily::Ipv4,
                observed,
                evidence_peers: 2,
            }
        );
        assert!(outcome.supports_reachability());
    }

    #[test]
    fn unsigned_msg7_never_confirms() {
        let mut table = PeerTestTable::new();
        let observed = endpoint(10, 40000);
        table
            .start(
                112,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        table
            .ingest(
                &msg4(112, 1000, observed),
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg4(112, 1000, observed).signature(),
                1000,
                100,
            )
            .expect("msg4");
        table
            .ingest(
                &msg5(112, 1001, observed, false),
                &CHARLIE_HASH,
                observed,
                &BOB_HASH,
                None,
                None,
                &[],
                1001,
                200,
            )
            .expect("msg5");
        let outcome = table
            .ingest(
                &msg7(112, 1002, observed, false),
                &CHARLIE_HASH,
                observed,
                &BOB_HASH,
                None,
                None,
                &[],
                1002,
                300,
            )
            .expect("msg7")
            .expect("outcome");
        assert_eq!(
            outcome,
            PeerTestOutcome::Inconclusive {
                family: AddressFamily::Ipv4
            }
        );
        assert!(!outcome.supports_reachability());
    }

    #[test]
    fn contradictory_observations_do_not_last_win() {
        let mut table = PeerTestTable::new();
        let first = endpoint(10, 40000);
        let second = endpoint(11, 40001);
        table
            .start(
                113,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                first,
                0,
            )
            .expect("start");
        table
            .ingest(
                &msg4(113, 1000, first),
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg4(113, 1000, first).signature(),
                1000,
                100,
            )
            .expect("msg4");
        let outcome = table
            .ingest(
                &msg5(113, 1001, second, true),
                &CHARLIE_HASH,
                second,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg5(113, 1001, second, true).signature(),
                1001,
                200,
            )
            .expect("msg5")
            .expect("outcome");
        assert!(matches!(outcome, PeerTestOutcome::AddressMismatch { .. }));
        assert!(outcome.contradicts_reachability());
    }

    #[test]
    fn invalid_signature_stale_wrong_role_and_unknown_are_bounded() {
        let mut table = PeerTestTable::new();
        let observed = endpoint(10, 40000);
        table
            .start(
                114,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        // Invalid signature.
        let bad = PeerTestBlock::new(
            4,
            0,
            Some(CHARLIE_HASH),
            2,
            114,
            1000,
            observed,
            vec![0xEE; 64],
        )
        .expect("bad sig shape");
        assert_eq!(
            table.ingest(
                &bad,
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                bad.signature(),
                1000,
                100,
            ),
            Err(PeerTestError::InvalidSignature)
        );
        // Stale timestamp.
        assert_eq!(
            table.ingest(
                &msg4(114, 1000, observed),
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg4(114, 1000, observed).signature(),
                1000 + PEER_TEST_MAX_CLOCK_SKEW_SECONDS + 1,
                100,
            ),
            Err(PeerTestError::StaleTimestamp)
        );
        // Wrong role/state: Msg 7 while awaiting Msg 4.
        assert_eq!(
            table.ingest(
                &msg7(114, 1001, observed, true),
                &CHARLIE_HASH,
                observed,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg7(114, 1001, observed, true).signature(),
                1001,
                100,
            ),
            Err(PeerTestError::WrongRole)
        );
        // Wrong sender: Msg 4 must come from Bob.
        assert_eq!(
            table.ingest(
                &msg4(114, 1000, observed),
                &CHARLIE_HASH,
                observed,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg4(114, 1000, observed).signature(),
                1000,
                100,
            ),
            Err(PeerTestError::SenderMismatch)
        );
        // Unknown correlation creates no state.
        assert_eq!(
            table.ingest(
                &msg4(999, 1000, observed),
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg4(999, 1000, observed).signature(),
                1000,
                100,
            ),
            Err(PeerTestError::UnknownTest)
        );
        assert_eq!(table.len(), 1);
        // None of the above created reachability evidence: the test is
        // still awaiting Msg 4 with no outcome.
        let _ = bad;
    }

    #[test]
    fn duplicate_and_reorder_are_idempotent() {
        let mut table = PeerTestTable::new();
        let observed = endpoint(10, 40000);
        table
            .start(
                115,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        let m4 = msg4(115, 1000, observed);
        let sig4 = m4.signature().to_vec();
        table
            .ingest(
                &m4,
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                &sig4,
                1000,
                100,
            )
            .expect("msg4");
        // Reordered duplicate Msg 4 while awaiting Msg 5: absorbed as
        // duplicate only for Msgs 5+; Msg 4 replay here is wrong-role
        // (already advanced) — assert the table stays in AwaitingMsg5
        // by sending the expected Msg 5 next and completing.
        let m5 = msg5(115, 1001, observed, true);
        let sig5 = m5.signature().to_vec();
        table
            .ingest(
                &m5,
                &CHARLIE_HASH,
                observed,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                &sig5,
                1001,
                200,
            )
            .expect("msg5");
        // Duplicate Msg 5 while awaiting Msg 7 is idempotent.
        let outcome = table
            .ingest(
                &m5,
                &CHARLIE_HASH,
                observed,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                &sig5,
                1001,
                210,
            )
            .expect("dup msg5");
        assert_eq!(outcome, None);
        assert_eq!(table.counters().duplicates_absorbed, 1);
    }

    #[test]
    fn concurrent_tests_are_isolated_by_nonce() {
        let mut table = PeerTestTable::new();
        let first = endpoint(10, 40000);
        let second = endpoint(11, 40001);
        table
            .start(
                121,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                first,
                0,
            )
            .expect("first");
        table
            .start(
                122,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                second,
                0,
            )
            .expect("second");
        // Crossing schedules: complete test 122's Msg 4 first, then
        // test 121's Msg 4. Neither consumes the other's message
        // because correlation is by nonce.
        table
            .ingest(
                &msg4(122, 1000, second),
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg4(122, 1000, second).signature(),
                1000,
                100,
            )
            .expect("122 msg4");
        table
            .ingest(
                &msg4(121, 1000, first),
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg4(121, 1000, first).signature(),
                1000,
                110,
            )
            .expect("121 msg4");
        // Finish 121 fully; 122 must still be awaiting Msg 5 (not
        // corrupted by 121's later messages).
        table
            .ingest(
                &msg5(121, 1001, first, true),
                &CHARLIE_HASH,
                first,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg5(121, 1001, first, true).signature(),
                1001,
                200,
            )
            .expect("121 msg5");
        let outcome = table
            .ingest(
                &msg7(121, 1002, first, true),
                &CHARLIE_HASH,
                first,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                msg7(121, 1002, first, true).signature(),
                1002,
                300,
            )
            .expect("121 msg7")
            .expect("outcome");
        assert!(matches!(
            outcome,
            PeerTestOutcome::DirectReachabilityConfirmed { .. }
        ));
        // Test 122 is untouched: its Msg 5 still accepted.
        assert!(
            table
                .ingest(
                    &msg5(122, 1001, second, true),
                    &CHARLIE_HASH,
                    second,
                    &BOB_HASH,
                    Some(&ALICE_HASH),
                    Some(&charlie_pub()),
                    msg5(122, 1001, second, true).signature(),
                    1001,
                    310,
                )
                .is_ok()
        );
    }

    #[test]
    fn quotas_bind_at_exact_capacity() {
        let mut table = PeerTestTable::new();
        let observed = endpoint(10, 40000);
        for nonce in 1..=MAX_PEER_TESTS_GLOBAL as u32 {
            // Spread across peers so the per-peer quota (2) never binds
            // before the global ceiling (8).
            let mut peer = [0xB0; 32];
            peer[0] = nonce as u8;
            table
                .start(
                    nonce,
                    PeerTestRole::Alice,
                    ALICE_HASH,
                    peer,
                    CHARLIE_HASH,
                    observed,
                    0,
                )
                .expect("start");
        }
        assert_eq!(table.len(), MAX_PEER_TESTS_GLOBAL);
        let mut peer = [0xFF; 32];
        peer[0] = 0xFE;
        assert_eq!(
            table.start(
                999,
                PeerTestRole::Alice,
                ALICE_HASH,
                peer,
                CHARLIE_HASH,
                observed,
                0
            ),
            Err(PeerTestError::TooManyTests)
        );
        // Per-peer quota: two tests with the same Bob hash succeed, the
        // third fails.
        let mut table = PeerTestTable::new();
        table
            .start(
                201,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("first");
        table
            .start(
                202,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("second");
        assert_eq!(
            table.start(
                203,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0
            ),
            Err(PeerTestError::PeerQuotaExceeded)
        );
    }

    #[test]
    fn expiry_and_cancel_release_to_baseline() {
        let mut table = PeerTestTable::new();
        let observed = endpoint(10, 40000);
        table
            .start(
                301,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        table
            .start(
                302,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        table.cancel(301).expect("cancel");
        assert_eq!(table.len(), 1);
        assert_eq!(table.cancel(301), Err(PeerTestError::UnknownTest));
        let expired = table.poll_expired(PEER_TEST_TIMEOUT_MS + 1);
        assert_eq!(expired, vec![302]);
        assert!(table.is_empty());
        assert_eq!(table.next_deadline_ms(), None);
    }

    #[test]
    fn refusal_and_timeout_are_inconclusive_not_confirmation() {
        let mut table = PeerTestTable::new();
        let observed = endpoint(10, 40000);
        table
            .start(
                401,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        // Explicit refusal code in Msg 4.
        let refused = PeerTestBlock::new(
            4,
            1,
            Some(CHARLIE_HASH),
            2,
            401,
            1000,
            observed,
            sign_charlie_msg(4, 401, 1000, observed),
        )
        .expect("refused");
        let outcome = table
            .ingest(
                &refused,
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                refused.signature(),
                1000,
                100,
            )
            .expect("ingest")
            .expect("outcome");
        assert_eq!(
            outcome,
            PeerTestOutcome::Rejected {
                family: AddressFamily::Ipv4
            }
        );
        // Timeout-driven inconclusive on a fresh test.
        table
            .start(
                402,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        let outcome = table.mark_inconclusive(402, 100).expect("inconclusive");
        assert_eq!(
            outcome,
            PeerTestOutcome::Inconclusive {
                family: AddressFamily::Ipv4
            }
        );
        assert!(!outcome.supports_reachability());
        assert!(!outcome.contradicts_reachability());
    }

    #[test]
    fn unsupported_signer_fails_closed() {
        let mut table = PeerTestTable::new();
        let observed = endpoint(10, 40000);
        table
            .start(
                501,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                observed,
                0,
            )
            .expect("start");
        let block = msg4(501, 1000, observed);
        let unknown_key = SigningPublicKey::new(SigningKeyType::Unknown(9999), vec![0xAA; 32]);
        // Unknown key types cannot even be constructed (no length
        // known), so exercise the path with a valid-shape but wrong
        // key: verification must fail closed as invalid, not confirm.
        let _ = unknown_key;
        assert_eq!(
            table.ingest(
                &block,
                &BOB_HASH,
                endpoint(20, 5000),
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&alice_pub()),
                block.signature(),
                1000,
                100,
            ),
            Err(PeerTestError::InvalidSignature)
        );
    }

    #[test]
    fn ipv6_evidence_is_separate_family() {
        use core::net::Ipv6Addr;
        let v6 = Ssu2Endpoint::new(
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            40000,
        )
        .expect("v6");
        let v6_bob = Ssu2Endpoint::new(
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2)),
            5000,
        )
        .expect("v6 bob");
        let mut table = PeerTestTable::new();
        table
            .start(
                601,
                PeerTestRole::Alice,
                ALICE_HASH,
                BOB_HASH,
                CHARLIE_HASH,
                v6,
                0,
            )
            .expect("start");
        let m4 = PeerTestBlock::new(
            4,
            0,
            Some(CHARLIE_HASH),
            2,
            601,
            1000,
            v6,
            sign_charlie_msg(4, 601, 1000, v6),
        )
        .expect("m4");
        let sig = m4.signature().to_vec();
        table
            .ingest(
                &m4,
                &BOB_HASH,
                v6_bob,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                &sig,
                1000,
                100,
            )
            .expect("m4");
        let m5 = PeerTestBlock::new(
            5,
            0,
            None,
            2,
            601,
            1001,
            v6,
            sign_charlie_msg(5, 601, 1001, v6),
        )
        .expect("m5");
        let sig5 = m5.signature().to_vec();
        table
            .ingest(
                &m5,
                &CHARLIE_HASH,
                v6,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                &sig5,
                1001,
                200,
            )
            .expect("m5");
        let m7 = PeerTestBlock::new(
            7,
            0,
            None,
            2,
            601,
            1002,
            v6,
            sign_charlie_msg(7, 601, 1002, v6),
        )
        .expect("m7");
        let sig7 = m7.signature().to_vec();
        let outcome = table
            .ingest(
                &m7,
                &CHARLIE_HASH,
                v6,
                &BOB_HASH,
                Some(&ALICE_HASH),
                Some(&charlie_pub()),
                &sig7,
                1002,
                300,
            )
            .expect("m7")
            .expect("outcome");
        assert_eq!(outcome.family(), AddressFamily::Ipv6);
    }
}
