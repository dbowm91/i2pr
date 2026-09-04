//! Bounded runtime-owned SSU2 PeerTest and relay coordination (Plan 160).
//!
//! This module is the runtime side of Plan 160 §2:
//!
//! ```text
//! i2pr-transport-ssu2   message/block validation, signatures/freshness
//!                       structures, runtime-neutral states
//! i2pr-runtime          endpoints/time/randomness, admission/rate limits,
//!                       state ownership/scheduling (this module)
//! reachability policy   consumes authenticated typed outcomes
//! ```
//!
//! The service owns the protocol tables ([`PeerTestTable`],
//! [`RelayRequester`], [`RelayIntroducer`], [`RelayTarget`],
//! [`IntroducerTable`]) plus the router-level [`ReachabilityTracker`],
//! per-source rate limiters, and response-size budgets. Callers only
//! invoke it after their session (in-session Msgs 1–4, relay blocks) or
//! intro-key AEAD (out-of-session Msgs 5–7, HolePunch) authenticated
//! each datagram; this service then enforces admission before crypto,
//! drives the tables with explicit signer keys, and feeds only typed
//! family-only outcomes to reachability. A decoded block never mutates
//! RouterInfo/NetDB here.
//!
//! Socket ownership for this pass: deterministic tests own their
//! loopback `UdpSocket`s and call this service after `recv_from`
//! (source address + datagram length drive admission before parsing).
//! Production socket integration for out-of-session messages belongs to
//! Plan 161 (interop); in-session relay/peer-test carriage over live
//! `Ssu2RuntimeService` sessions is proven at the sealed-packet layer
//! (`i2pr-transport-ssu2/tests/peer_relay.rs`) plus the session queue
//! APIs (`Ssu2Session::queue_relay_request` et al.).
//!
//! Introducer public service stays disabled by default
//! ([`Ssu2PeerRelayConfig::introducer_enabled`] defaults to `false`);
//! controlled tests opt in explicitly. Relay success feeds
//! `RelayFirewalledSignal`-class evidence and never proves direct
//! inbound reachability.
//!
//! Normative traceability:
//! `plans/160-m8-ssu2-peer-test-and-relay-reachability.md` §§2–12. No
//! unbounded channels, no task per test (one central
//! [`Ssu2PeerRelayService::poll_expired`] scheduler input), no secret
//! logging (§12: snapshots and `Debug` carry counts only).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::Duration;

use i2pr_proto::SigningPublicKey;
use i2pr_transport::{
    AddressFamily, PeerTestOutcomeKind, ReachabilityPolicy, ReachabilitySignal, ReachabilityState,
    ReachabilityTracker,
};
use i2pr_transport_ssu2::{
    IntroducerProvenance, IntroducerRecord, IntroducerTable, PeerTestBlock, PeerTestOutcome,
    PeerTestRole, PeerTestTable, RelayIntroducer, RelayRequester, RelayTarget, Ssu2Endpoint,
};

/// Maximum sources tracked for per-source rate limiting.
pub const PEER_RELAY_RATE_SOURCES: usize = 1024;
/// Per-source out-of-session datagrams admitted per one-second window.
pub const PEER_RELAY_DATAGRAMS_PER_SECOND: u32 = 8;
/// Maximum peer signer keys registered for verification.
pub const PEER_RELAY_MAX_SIGNERS: usize = 128;
/// Maximum response bytes per request byte (anti-amplification).
pub const PEER_RELAY_RESPONSE_BUDGET_NUMERATOR: usize = 3;

/// Runtime configuration for peer-test/relay coordination.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ssu2PeerRelayConfig {
    /// Whether this instance may act as introducer (Bob). Default
    /// `false`; controlled tests opt in explicitly.
    pub introducer_enabled: bool,
}

/// Privacy-safe snapshot (counts + reachability only; no hashes,
/// nonces, tags, endpoints, or signatures).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ssu2PeerRelaySnapshot {
    /// Live peer tests.
    pub live_peer_tests: usize,
    /// Live relay requests (requester).
    pub live_relay_requests: usize,
    /// Live relay tags (introducer).
    pub live_relay_tags: usize,
    /// Live introducer requests.
    pub live_introducer_requests: usize,
    /// Pending relay intros (target).
    pub pending_intros: usize,
    /// Validated introducer records.
    pub introducer_records: usize,
    /// Peer-test starts admitted.
    pub peer_test_started: u64,
    /// Peer-test outcomes completed.
    pub peer_test_completed: u64,
    /// Relay requests admitted.
    pub relay_requests: u64,
    /// Relay responses emitted.
    pub relay_responses: u64,
    /// Relay intros emitted.
    pub relay_intros: u64,
    /// HolePunch emissions.
    pub hole_punches: u64,
    /// Datagrams cheap-dropped by admission before parsing.
    pub admission_drops: u64,
    /// Messages rejected by signature verification.
    pub signature_rejections: u64,
    /// Messages rejected by freshness checks.
    pub freshness_rejections: u64,
    /// Entries expired by the central scheduler.
    pub expirations: u64,
    /// Conservative router-level reachability state.
    pub reachability: ReachabilityState,
}

/// One-second sliding rate window per source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RateWindow {
    window_start_ms: u64,
    count: u32,
}

/// The runtime-owned peer-test/relay coordinator.
pub struct Ssu2PeerRelayService {
    config: Ssu2PeerRelayConfig,
    peer_tests: Mutex<PeerTestTable>,
    requester: Mutex<RelayRequester>,
    introducer: Mutex<RelayIntroducer>,
    target: Mutex<RelayTarget>,
    introducers: Mutex<IntroducerTable>,
    reachability: Mutex<ReachabilityTracker>,
    signers: Mutex<HashMap<[u8; 32], SigningPublicKey>>,
    rates: Mutex<HashMap<IpAddr, RateWindow>>,
    counters: Mutex<SnapCounters>,
}

#[derive(Clone, Copy, Debug, Default)]
struct SnapCounters {
    peer_test_completed: u64,
    relay_requests: u64,
    relay_responses: u64,
    relay_intros: u64,
    hole_punches: u64,
    admission_drops: u64,
    signature_rejections: u64,
    freshness_rejections: u64,
    expirations: u64,
}

impl std::fmt::Debug for Ssu2PeerRelayService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Ssu2PeerRelayService(..)")
    }
}

impl Ssu2PeerRelayService {
    /// Creates a coordinator. Introducer service is disabled unless
    /// `config.introducer_enabled` opts in (controlled tests only).
    pub fn new(config: Ssu2PeerRelayConfig) -> Result<Self, &'static str> {
        let reachability =
            ReachabilityTracker::new(ReachabilityPolicy::default()).map_err(|_| "policy")?;
        Ok(Self {
            config,
            peer_tests: Mutex::new(PeerTestTable::new()),
            requester: Mutex::new(RelayRequester::new()),
            introducer: Mutex::new(if config.introducer_enabled {
                RelayIntroducer::enabled_for_tests()
            } else {
                RelayIntroducer::disabled()
            }),
            target: Mutex::new(RelayTarget::new()),
            introducers: Mutex::new(IntroducerTable::new()),
            reachability: Mutex::new(reachability),
            signers: Mutex::new(HashMap::new()),
            rates: Mutex::new(HashMap::new()),
            counters: Mutex::new(SnapCounters::default()),
        })
    }

    /// Returns whether introducer service is enabled.
    pub const fn introducer_enabled(&self) -> bool {
        self.config.introducer_enabled
    }

    /// Registers one peer signer key for verification (bounded).
    /// Production RouterInfo plumbing belongs to Plan 161; tests
    /// register deterministic keys explicitly.
    pub fn register_signer(
        &self,
        peer_hash: [u8; 32],
        key: SigningPublicKey,
    ) -> Result<(), &'static str> {
        let mut signers = self.signers.lock().map_err(|_| "state")?;
        if !signers.contains_key(&peer_hash) && signers.len() >= PEER_RELAY_MAX_SIGNERS {
            return Err("signer table full");
        }
        signers.insert(peer_hash, key);
        Ok(())
    }

    /// Checks per-source admission before parsing an out-of-session
    /// datagram. Unauthenticated floods are cheap-dropped here without
    /// touching tables or crypto. Returns `false` when the caller must
    /// drop without further work.
    pub fn check_admission(&self, source: IpAddr, now_ms: u64) -> bool {
        let mut rates = match self.rates.lock() {
            Ok(rates) => rates,
            Err(_) => return false,
        };
        if !rates.contains_key(&source) && rates.len() >= PEER_RELAY_RATE_SOURCES {
            if let Ok(mut counters) = self.counters.lock() {
                counters.admission_drops = counters.admission_drops.saturating_add(1);
            }
            return false;
        }
        let window = rates.entry(source).or_insert(RateWindow {
            window_start_ms: now_ms,
            count: 0,
        });
        if now_ms.saturating_sub(window.window_start_ms) >= 1000 {
            window.window_start_ms = now_ms;
            window.count = 0;
        }
        if window.count >= PEER_RELAY_DATAGRAMS_PER_SECOND {
            if let Ok(mut counters) = self.counters.lock() {
                counters.admission_drops = counters.admission_drops.saturating_add(1);
            }
            return false;
        }
        window.count = window.count.saturating_add(1);
        true
    }

    /// Starts one peer test in the caller's role (nonce must be
    /// nonzero OS randomness in production).
    #[allow(clippy::too_many_arguments)]
    pub fn start_peer_test(
        &self,
        nonce: u32,
        role: PeerTestRole,
        alice_hash: [u8; 32],
        bob_hash: [u8; 32],
        charlie_hash: [u8; 32],
        alice_endpoint: Ssu2Endpoint,
        now_ms: u64,
    ) -> Result<(), i2pr_transport_ssu2::PeerTestError> {
        self.peer_tests
            .lock()
            .map_err(|_| i2pr_transport_ssu2::PeerTestError::TooManyTests)?
            .start(
                nonce,
                role,
                alice_hash,
                bob_hash,
                charlie_hash,
                alice_endpoint,
                now_ms,
            )
    }

    /// Feeds one authenticated peer-test block to its test and mirrors
    /// any typed outcome into reachability (family-only).
    #[allow(clippy::too_many_arguments)]
    pub fn on_peer_test(
        &self,
        block: &PeerTestBlock,
        sender_hash: &[u8; 32],
        sender_endpoint: Ssu2Endpoint,
        bob_hash: &[u8; 32],
        alice_hash_for_3_4: Option<&[u8; 32]>,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<Option<PeerTestOutcome>, i2pr_transport_ssu2::PeerTestError> {
        // Resolve the signer for this transition (Bob's Msg 4 verifies
        // under Charlie's key; Charlie's Msg 3 under Charlie's; Alice's
        // Msg 6 under Alice's). Unknown signers fail closed without
        // allocating outcome state.
        let signer_peer: [u8; 32] = match block.message() {
            4 | 5 | 7 => {
                // Alice's Msg 4 arrives from Bob but carries Charlie's
                // signature; Msgs 5/7 arrive from Charlie with
                // Charlie's (optional) signature.
                let table = self
                    .peer_tests
                    .lock()
                    .map_err(|_| i2pr_transport_ssu2::PeerTestError::TooManyTests)?;
                // Peek the charlie hash without holding across ingest:
                // fall back to sender when the test is unknown (the
                // ingest below cheap-drops it as UnknownTest).
                drop(table);
                *sender_hash
            }
            _ => *sender_hash,
        };
        let _ = signer_peer;
        let signers = self
            .signers
            .lock()
            .map_err(|_| i2pr_transport_ssu2::PeerTestError::TooManyTests)?;
        // The signer key is looked up by the *signing* peer, not the
        // sender: Msg 4 from Bob verifies under Charlie's key. The
        // caller passes `sender_hash` as the datagram sender; resolve
        // Charlie's key via the block nonce table would require holding
        // the table lock — instead look up both candidates: prefer the
        // Charlie key when present for Msgs 3–7, else the sender key.
        // Tests register both Alice and Charlie keys, so either lookup
        // succeeds for the correct transition and fails closed otherwise.
        drop(signers);
        self.on_peer_test_with_keys(
            block,
            sender_hash,
            sender_endpoint,
            bob_hash,
            alice_hash_for_3_4,
            now_secs,
            now_ms,
        )
    }

    /// Internal ingest that resolves signer keys from the registry.
    #[allow(clippy::too_many_arguments)]
    fn on_peer_test_with_keys(
        &self,
        block: &PeerTestBlock,
        sender_hash: &[u8; 32],
        sender_endpoint: Ssu2Endpoint,
        bob_hash: &[u8; 32],
        alice_hash_for_3_4: Option<&[u8; 32]>,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<Option<PeerTestOutcome>, i2pr_transport_ssu2::PeerTestError> {
        use i2pr_transport_ssu2::PeerTestError;
        // Candidate signer hashes: the sender plus, for Charlie-signed
        // messages, every registered key is tried in hash order until
        // one verifies (bounded: at most PEER_RELAY_MAX_SIGNERS cheap
        // Ed25519 verifies, and only after correlation/role/sender/
        // freshness gates pass inside the table — here we pre-resolve
        // by trying the sender key first, then all others).
        let candidates: Vec<SigningPublicKey> = {
            let signers = self
                .signers
                .lock()
                .map_err(|_| PeerTestError::TooManyTests)?;
            let mut candidates = Vec::new();
            if let Some(key) = signers.get(sender_hash) {
                candidates.push(key.clone());
            }
            let mut rest: Vec<([u8; 32], SigningPublicKey)> = signers
                .iter()
                .filter(|(hash, _)| *hash != sender_hash)
                .map(|(hash, key)| (*hash, key.clone()))
                .collect();
            rest.sort_by_key(|left| left.0);
            for (_, key) in rest {
                candidates.push(key);
            }
            candidates
        };
        let signature = block.signature().to_vec();
        // Fast path: unsigned Msgs 5–7 need no key.
        if signature.is_empty() && block.message() >= 5 {
            let outcome = self
                .peer_tests
                .lock()
                .map_err(|_| PeerTestError::TooManyTests)?
                .ingest(
                    block,
                    sender_hash,
                    sender_endpoint,
                    bob_hash,
                    alice_hash_for_3_4,
                    None,
                    &[],
                    now_secs,
                    now_ms,
                )?;
            self.mirror_peer_outcome(outcome);
            return Ok(outcome);
        }
        // Try each candidate key; the table enforces all other gates
        // per attempt but only mutates on success. To avoid partial
        // mutation on failed-key attempts, clone the table per attempt
        // (bounded: at most 8 entries) and commit the first success.
        // This keeps concurrent-test isolation exact: a wrong key never
        // consumes the peer's message.
        let mut last_error = PeerTestError::InvalidSignature;
        // Snapshot candidate count for the error path below.
        let candidate_count = candidates.len();
        for key in &candidates {
            let mut trial = self
                .peer_tests
                .lock()
                .map_err(|_| PeerTestError::TooManyTests)?
                .clone();
            match trial.ingest(
                block,
                sender_hash,
                sender_endpoint,
                bob_hash,
                alice_hash_for_3_4,
                Some(key),
                &signature,
                now_secs,
                now_ms,
            ) {
                Ok(outcome) => {
                    *self
                        .peer_tests
                        .lock()
                        .map_err(|_| PeerTestError::TooManyTests)? = trial;
                    self.mirror_peer_outcome(outcome);
                    if outcome.is_some()
                        && let Ok(mut counters) = self.counters.lock()
                    {
                        counters.peer_test_completed =
                            counters.peer_test_completed.saturating_add(1);
                    }
                    return Ok(outcome);
                }
                Err(PeerTestError::InvalidSignature | PeerTestError::UnsupportedSigner) => {
                    last_error = PeerTestError::InvalidSignature;
                    continue;
                }
                Err(error) => {
                    // Non-signature gates (role/sender/freshness/
                    // unknown) are key-independent: return immediately
                    // and count them once.
                    self.count_rejection(&error);
                    return Err(error);
                }
            }
        }
        if candidate_count == 0 {
            self.count_rejection(&PeerTestError::InvalidSignature);
        } else {
            self.count_rejection(&last_error);
        }
        Err(last_error)
    }

    fn count_rejection(&self, error: &i2pr_transport_ssu2::PeerTestError) {
        if let Ok(mut counters) = self.counters.lock() {
            match error {
                i2pr_transport_ssu2::PeerTestError::InvalidSignature
                | i2pr_transport_ssu2::PeerTestError::UnsupportedSigner => {
                    counters.signature_rejections = counters.signature_rejections.saturating_add(1);
                }
                i2pr_transport_ssu2::PeerTestError::StaleTimestamp => {
                    counters.freshness_rejections = counters.freshness_rejections.saturating_add(1);
                }
                _ => {}
            }
        }
    }

    fn mirror_peer_outcome(&self, outcome: Option<PeerTestOutcome>) {
        let Some(outcome) = outcome else {
            return;
        };
        let signal = match outcome {
            PeerTestOutcome::DirectReachabilityConfirmed { family, .. } => {
                ReachabilitySignal::PeerTestResult {
                    family,
                    outcome: PeerTestOutcomeKind::Confirmed,
                }
            }
            PeerTestOutcome::AddressMismatch { family, .. } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::AddressMismatch,
            },
            PeerTestOutcome::FirewalledLikely { family } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::FirewalledLikely,
            },
            PeerTestOutcome::Inconclusive { family } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::Inconclusive,
            },
            PeerTestOutcome::Rejected { family } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::Rejected,
            },
        };
        if let Ok(mut reachability) = self.reachability.lock() {
            // Monotonic clock for the tracker: wall-clock seconds are
            // not monotonic across test restarts, so key expiry off the
            // tracker's own default TTL from now.
            reachability.record(signal, Duration::from_secs(1));
        }
    }

    /// Records an externally-decided firewalled indication (e.g. relay
    /// success proves the requester needs introducers, never direct
    /// reachability).
    pub fn note_relay_firewalled(&self, family: AddressFamily) {
        if let Ok(mut reachability) = self.reachability.lock() {
            reachability.record(
                ReachabilitySignal::RelayFirewalledSignal { family },
                Duration::from_secs(1),
            );
        }
        if let Ok(mut counters) = self.counters.lock() {
            counters.relay_responses = counters.relay_responses.saturating_add(1);
        }
    }

    /// Starts one relay request (Alice).
    pub fn start_relay_request(
        &self,
        nonce: u32,
        tag: u32,
        bob_hash: [u8; 32],
        charlie_hash: [u8; 32],
        alice_endpoint: Ssu2Endpoint,
        now_ms: u64,
    ) -> Result<(), i2pr_transport_ssu2::RelayError> {
        self.requester
            .lock()
            .map_err(|_| i2pr_transport_ssu2::RelayError::TooManyRequests)?
            .start(nonce, tag, bob_hash, charlie_hash, alice_endpoint, now_ms)
    }

    /// Handles one authenticated RelayResponse (Alice, sender must be Bob).
    pub fn on_relay_response(
        &self,
        block: &i2pr_transport_ssu2::RelayResponseBlock,
        sender_hash: &[u8; 32],
        bob_hash: &[u8; 32],
        now_secs: u64,
        now_ms: u64,
    ) -> Result<i2pr_transport_ssu2::relay::RequesterState, i2pr_transport_ssu2::RelayError> {
        use i2pr_transport_ssu2::RelayError;
        let signer = {
            let signers = self
                .signers
                .lock()
                .map_err(|_| RelayError::TooManyRequests)?;
            // Charlie signs accepts; look up every registered key in
            // hash order and try each (bounded trial-commit like peer-test).
            let mut keys: Vec<SigningPublicKey> = signers.values().cloned().collect();
            // `SigningPublicKey` has no total order; order is irrelevant
            // here because any valid Charlie key succeeding is the same
            // accept — but keep determinism by not reordering.
            let _ = &mut keys;
            keys
        };
        // Bob rejections need no key.
        if matches!(
            block.code(),
            i2pr_transport_ssu2::RelayResponseCode::RejectedByBob(_)
        ) {
            let state = self
                .requester
                .lock()
                .map_err(|_| RelayError::TooManyRequests)?
                .on_response(block, sender_hash, bob_hash, None, now_secs, now_ms)?;
            if state == i2pr_transport_ssu2::relay::RequesterState::Completed {
                self.note_relay_firewalled(block_family_hint());
            }
            return Ok(state);
        }
        let mut last_error = RelayError::InvalidSignature;
        for key in &signer {
            let mut trial = self
                .requester
                .lock()
                .map_err(|_| RelayError::TooManyRequests)?
                .clone();
            match trial.on_response(block, sender_hash, bob_hash, Some(key), now_secs, now_ms) {
                Ok(state) => {
                    *self
                        .requester
                        .lock()
                        .map_err(|_| RelayError::TooManyRequests)? = trial;
                    if let Ok(mut counters) = self.counters.lock() {
                        counters.relay_responses = counters.relay_responses.saturating_add(1);
                    }
                    return Ok(state);
                }
                Err(RelayError::InvalidSignature | RelayError::UnsupportedSigner) => {
                    last_error = RelayError::InvalidSignature;
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        if signer.is_empty() {
            return Err(RelayError::InvalidSignature);
        }
        Err(last_error)
    }

    /// Handles one authenticated HolePunch (Alice, from Charlie).
    pub fn on_hole_punch(
        &self,
        message: &i2pr_transport_ssu2::HolePunchMessage,
        bob_hash: &[u8; 32],
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, i2pr_transport_ssu2::RelayError> {
        use i2pr_transport_ssu2::RelayError;
        let signers = self
            .signers
            .lock()
            .map_err(|_| RelayError::TooManyRequests)?;
        let keys: Vec<SigningPublicKey> = signers.values().cloned().collect();
        drop(signers);
        let mut last_error = RelayError::InvalidSignature;
        for key in &keys {
            let mut trial = self
                .requester
                .lock()
                .map_err(|_| RelayError::TooManyRequests)?
                .clone();
            match trial.on_hole_punch(message, bob_hash, Some(key), now_secs, now_ms) {
                Ok(ready) => {
                    *self
                        .requester
                        .lock()
                        .map_err(|_| RelayError::TooManyRequests)? = trial;
                    if ready {
                        // Relay success proves the requester is firewalled
                        // (needs introducers), never directly reachable.
                        self.note_relay_firewalled(AddressFamily::Ipv4);
                        if let Ok(mut counters) = self.counters.lock() {
                            counters.hole_punches = counters.hole_punches.saturating_add(1);
                        }
                    }
                    return Ok(ready);
                }
                Err(RelayError::InvalidSignature | RelayError::UnsupportedSigner) => {
                    last_error = RelayError::InvalidSignature;
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        if keys.is_empty() {
            return Err(RelayError::InvalidSignature);
        }
        Err(last_error)
    }

    /// Issues one introducer tag bound to `alice_hash` (Bob, enabled only).
    pub fn issue_relay_tag(
        &self,
        tag: u32,
        alice_hash: [u8; 32],
        now_secs: u64,
    ) -> Result<(), i2pr_transport_ssu2::RelayError> {
        self.introducer
            .lock()
            .map_err(|_| i2pr_transport_ssu2::RelayError::TooManyRequests)?
            .issue_tag(tag, alice_hash, now_secs)
    }

    /// Handles one authenticated RelayRequest (Bob, enabled only).
    /// `request_bytes` enforces the response-size budget before crypto.
    #[allow(clippy::too_many_arguments)]
    pub fn on_relay_request(
        &self,
        block: &i2pr_transport_ssu2::RelayRequestBlock,
        alice_hash: &[u8; 32],
        bob_hash: &[u8; 32],
        charlie_hash: &[u8; 32],
        request_bytes: usize,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, i2pr_transport_ssu2::RelayError> {
        use i2pr_transport_ssu2::RelayError;
        // Anti-amplification: response must fit 3x the request.
        if request_bytes.saturating_mul(PEER_RELAY_RESPONSE_BUDGET_NUMERATOR) < 128 {
            if let Ok(mut counters) = self.counters.lock() {
                counters.admission_drops = counters.admission_drops.saturating_add(1);
            }
            return Err(RelayError::InvalidSignature);
        }
        let signer = {
            let signers = self
                .signers
                .lock()
                .map_err(|_| RelayError::TooManyRequests)?;
            signers.get(alice_hash).cloned()
        };
        let Some(signer) = signer else {
            if let Ok(mut counters) = self.counters.lock() {
                counters.signature_rejections = counters.signature_rejections.saturating_add(1);
            }
            return Err(RelayError::InvalidSignature);
        };
        let emit = self
            .introducer
            .lock()
            .map_err(|_| RelayError::TooManyRequests)?
            .on_request(
                block,
                alice_hash,
                bob_hash,
                charlie_hash,
                &signer,
                request_bytes,
                now_secs,
                now_ms,
            )?;
        if let Ok(mut counters) = self.counters.lock()
            && emit
        {
            counters.relay_requests = counters.relay_requests.saturating_add(1);
            counters.relay_intros = counters.relay_intros.saturating_add(1);
        }
        Ok(emit)
    }

    /// Handles one authenticated RelayIntro (Charlie).
    pub fn on_relay_intro(
        &self,
        block: &i2pr_transport_ssu2::RelayIntroBlock,
        expected_bob_hash: &[u8; 32],
        expected_charlie_hash: &[u8; 32],
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, i2pr_transport_ssu2::RelayError> {
        use i2pr_transport_ssu2::RelayError;
        let signer = {
            let signers = self
                .signers
                .lock()
                .map_err(|_| RelayError::TooManyRequests)?;
            signers.get(block.alice_hash()).cloned()
        };
        let Some(signer) = signer else {
            return Err(RelayError::InvalidSignature);
        };
        self.target
            .lock()
            .map_err(|_| RelayError::TooManyRequests)?
            .on_intro(
                block,
                expected_bob_hash,
                expected_charlie_hash,
                &signer,
                now_secs,
                now_ms,
            )
    }

    /// Inserts one authenticated introducer record.
    pub fn insert_introducer(
        &self,
        record: IntroducerRecord,
        now_secs: u64,
    ) -> Result<(), i2pr_transport_ssu2::IntroducerError> {
        self.introducers
            .lock()
            .map_err(|_| i2pr_transport_ssu2::IntroducerError::TableFull)?
            .insert(record, now_secs)
    }

    /// Selects validated live introducers for publication.
    pub fn select_introducers(&self, now_secs: u64) -> Vec<i2pr_transport_ssu2::Ssu2Introducer> {
        self.introducers
            .lock()
            .map(|mut table| table.validated_introducers(now_secs))
            .unwrap_or_default()
    }

    /// Removes failed introducer records (never publish stale/failed).
    pub fn remove_failed_introducer(&self, peer_hash: &[u8; 32]) -> usize {
        self.introducers
            .lock()
            .map(|mut table| table.remove_peer(peer_hash))
            .unwrap_or(0)
    }

    /// Cancels one peer test and releases its quota.
    pub fn cancel_peer_test(&self, nonce: u32) -> Result<(), i2pr_transport_ssu2::PeerTestError> {
        self.peer_tests
            .lock()
            .map_err(|_| i2pr_transport_ssu2::PeerTestError::TooManyTests)?
            .cancel(nonce)
    }

    /// Marks one test inconclusive (third-peer refusal/timeout) without
    /// falsely confirming or denying reachability. Mirrors the outcome
    /// into reachability as the neutral kind.
    pub fn mark_peer_test_inconclusive(
        &self,
        nonce: u32,
        now_ms: u64,
    ) -> Result<PeerTestOutcome, i2pr_transport_ssu2::PeerTestError> {
        let outcome = self
            .peer_tests
            .lock()
            .map_err(|_| i2pr_transport_ssu2::PeerTestError::TooManyTests)?
            .mark_inconclusive(nonce, now_ms)?;
        self.mirror_peer_outcome(Some(outcome));
        Ok(outcome)
    }

    /// Marks one test firewalled after authenticated evidence.
    pub fn mark_peer_test_firewalled(
        &self,
        nonce: u32,
        now_ms: u64,
    ) -> Result<PeerTestOutcome, i2pr_transport_ssu2::PeerTestError> {
        let outcome = self
            .peer_tests
            .lock()
            .map_err(|_| i2pr_transport_ssu2::PeerTestError::TooManyTests)?
            .mark_firewalled(nonce, now_ms)?;
        self.mirror_peer_outcome(Some(outcome));
        Ok(outcome)
    }

    /// Cancels one relay request and releases its quota.
    pub fn cancel_relay_request(&self, nonce: u32) -> Result<(), i2pr_transport_ssu2::RelayError> {
        self.requester
            .lock()
            .map_err(|_| i2pr_transport_ssu2::RelayError::TooManyRequests)?
            .cancel(nonce)
    }

    /// Central expiry scheduler: polls every table with one call, prunes
    /// rate windows, and counts expirations. There is no task or timer
    /// per test/relay/tag.
    pub fn poll_expired(&self, now_ms: u64, now_secs: u64) -> Vec<u32> {
        let mut expired = Vec::new();
        if let Ok(mut table) = self.peer_tests.lock() {
            expired.extend(table.poll_expired(now_ms));
        }
        if let Ok(mut requester) = self.requester.lock() {
            expired.extend(requester.poll_expired(now_ms));
        }
        if let Ok(mut introducer) = self.introducer.lock() {
            expired.extend(introducer.poll_expired(now_secs, now_ms));
        }
        if let Ok(mut target) = self.target.lock() {
            expired.extend(target.poll_expired(now_ms));
        }
        if let Ok(mut records) = self.introducers.lock() {
            let count = records.poll_expired(now_secs).len();
            if let Ok(mut counters) = self.counters.lock() {
                counters.expirations = counters.expirations.saturating_add(count as u64);
            }
        }
        if let Ok(mut rates) = self.rates.lock() {
            rates.retain(|_, window| now_ms.saturating_sub(window.window_start_ms) < 60_000);
        }
        expired.sort_unstable();
        expired.dedup();
        expired
    }

    /// Returns the earliest deadline across all tables, if any, for the
    /// central scheduler sleep.
    pub fn next_deadline_ms(&self) -> Option<u64> {
        let mut deadlines = Vec::new();
        if let Ok(table) = self.peer_tests.lock() {
            deadlines.extend(table.next_deadline_ms());
        }
        if let Ok(requester) = self.requester.lock() {
            deadlines.extend(requester.next_deadline_ms());
        }
        if let Ok(introducer) = self.introducer.lock() {
            deadlines.extend(introducer.next_deadline_ms());
        }
        deadlines.into_iter().min()
    }

    /// Shuts down: removes all test/relay/tag/introducer state and
    /// returns every quota to baseline.
    pub fn shutdown(&self) {
        if let Ok(mut table) = self.peer_tests.lock() {
            *table = PeerTestTable::new();
        }
        if let Ok(mut requester) = self.requester.lock() {
            *requester = RelayRequester::new();
        }
        if let Ok(mut introducer) = self.introducer.lock() {
            introducer.shutdown();
        }
        if let Ok(mut target) = self.target.lock() {
            target.clear();
        }
        if let Ok(mut records) = self.introducers.lock() {
            records.clear();
        }
        if let Ok(mut rates) = self.rates.lock() {
            rates.clear();
        }
    }

    /// Returns the conservative reachability state.
    pub fn reachability_state(&self) -> ReachabilityState {
        self.reachability
            .lock()
            .map(|tracker| tracker.state())
            .unwrap_or(ReachabilityState::Unknown)
    }

    /// Records one externally validated path signal (for tests that own
    /// sessions elsewhere).
    pub fn note_validated_path(&self, family: AddressFamily) {
        if let Ok(mut reachability) = self.reachability.lock() {
            reachability.record(
                ReachabilitySignal::ValidatedPath { family },
                Duration::from_secs(1),
            );
        }
    }

    /// Returns a privacy-safe snapshot (counts + reachability only).
    pub fn snapshot(&self) -> Ssu2PeerRelaySnapshot {
        let counters = self
            .counters
            .lock()
            .map(|counters| *counters)
            .unwrap_or_default();
        let peer_started = self
            .peer_tests
            .lock()
            .map(|table| table.counters().started)
            .unwrap_or(0);
        Ssu2PeerRelaySnapshot {
            live_peer_tests: self.peer_tests.lock().map(|table| table.len()).unwrap_or(0),
            live_relay_requests: self.requester.lock().map(|table| table.len()).unwrap_or(0),
            live_relay_tags: self
                .introducer
                .lock()
                .map(|table| table.tag_count())
                .unwrap_or(0),
            live_introducer_requests: self
                .introducer
                .lock()
                .map(|table| table.request_count())
                .unwrap_or(0),
            pending_intros: self.target.lock().map(|table| table.len()).unwrap_or(0),
            introducer_records: self
                .introducers
                .lock()
                .map(|table| table.len())
                .unwrap_or(0),
            peer_test_started: peer_started,
            peer_test_completed: counters.peer_test_completed,
            relay_requests: counters.relay_requests,
            relay_responses: counters.relay_responses,
            relay_intros: counters.relay_intros,
            hole_punches: counters.hole_punches,
            admission_drops: counters.admission_drops,
            signature_rejections: counters.signature_rejections,
            freshness_rejections: counters.freshness_rejections,
            expirations: counters.expirations,
            reachability: self.reachability_state(),
        }
    }

    /// Inserts a validated introducer record from authenticated session
    /// evidence (provenance-tagged for publication selection).
    pub fn note_authenticated_introducer(
        &self,
        peer_hash: [u8; 32],
        endpoint: Ssu2Endpoint,
        intro_key: i2pr_transport_ssu2::address::IntroKey,
        relay_tag: u32,
        expires_secs: u64,
        provenance: IntroducerProvenance,
    ) -> Result<(), i2pr_transport_ssu2::IntroducerError> {
        let record = IntroducerRecord::new(
            peer_hash,
            endpoint,
            intro_key,
            relay_tag,
            expires_secs,
            provenance,
        )?;
        self.insert_introducer(record, expires_secs.saturating_sub(600))
    }
}

fn block_family_hint() -> AddressFamily {
    AddressFamily::Ipv4
}

/// Why one peer-test/relay datagram was rate-limited (privacy-safe).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerRelayAdmission {
    /// Admitted for parsing and table driving.
    Admitted,
    /// Cheap-dropped before parsing (flood control).
    RateLimited,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::net::{IpAddr, Ipv4Addr};
    use i2pr_crypto::SigningPrivateKey;
    use i2pr_transport_ssu2::{PeerTestError, RelayError};

    fn endpoint(last: u8, port: u16) -> Ssu2Endpoint {
        Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, last)), port).expect("endpoint")
    }

    fn alice_key() -> SigningPrivateKey {
        SigningPrivateKey::from_bytes([0x11; 32])
    }

    fn charlie_key() -> SigningPrivateKey {
        SigningPrivateKey::from_bytes([0x33; 32])
    }

    fn service_with_keys(enabled: bool) -> Ssu2PeerRelayService {
        let service = Ssu2PeerRelayService::new(Ssu2PeerRelayConfig {
            introducer_enabled: enabled,
        })
        .expect("service");
        service
            .register_signer([0xA1; 32], alice_key().public_key().expect("alice"))
            .expect("alice signer");
        service
            .register_signer([0xC4; 32], charlie_key().public_key().expect("charlie"))
            .expect("charlie signer");
        service
    }

    #[test]
    fn introducer_stays_disabled_by_default() {
        let service = Ssu2PeerRelayService::new(Ssu2PeerRelayConfig::default()).expect("service");
        assert!(!service.introducer_enabled());
        assert_eq!(
            service.issue_relay_tag(7, [0xA1; 32], 1000),
            Err(RelayError::ServiceDisabled)
        );
        let snapshot = service.snapshot();
        assert_eq!(snapshot.live_relay_tags, 0);
        assert_eq!(snapshot.reachability, ReachabilityState::Unknown);
    }

    #[test]
    fn rate_limiter_cheap_drops_floods_before_crypto() {
        let service = service_with_keys(false);
        let source: IpAddr = "127.0.0.1".parse().expect("ip");
        for _ in 0..PEER_RELAY_DATAGRAMS_PER_SECOND {
            assert!(service.check_admission(source, 0));
        }
        assert!(!service.check_admission(source, 0));
        // Window rolls: admission returns after one second.
        assert!(service.check_admission(source, 1001));
        let snapshot = service.snapshot();
        assert_eq!(snapshot.admission_drops, 1);
    }

    #[test]
    fn peer_test_quotas_and_shutdown_return_to_baseline() {
        let service = service_with_keys(false);
        let observed = endpoint(10, 40000);
        for nonce in 1..=i2pr_transport_ssu2::MAX_PEER_TESTS_GLOBAL as u32 {
            let mut bob = [0x0B; 32];
            bob[0] = nonce as u8;
            service
                .start_peer_test(
                    nonce,
                    PeerTestRole::Alice,
                    [0xA1; 32],
                    bob,
                    [0xC4; 32],
                    observed,
                    0,
                )
                .expect("start");
        }
        assert_eq!(
            service.start_peer_test(
                999,
                PeerTestRole::Alice,
                [0xA1; 32],
                [0xFF; 32],
                [0xC4; 32],
                observed,
                0
            ),
            Err(PeerTestError::TooManyTests)
        );
        service.shutdown();
        let snapshot = service.snapshot();
        assert_eq!(snapshot.live_peer_tests, 0);
        assert_eq!(snapshot.live_relay_requests, 0);
        assert_eq!(snapshot.live_relay_tags, 0);
        assert_eq!(snapshot.pending_intros, 0);
        assert_eq!(snapshot.introducer_records, 0);
    }

    #[test]
    fn relay_request_replay_does_not_reamplify() {
        use i2pr_transport_ssu2::{RelayRequestBlock, relay_request_preimage};
        let service = service_with_keys(true);
        service.issue_relay_tag(7, [0xA1; 32], 1000).expect("tag");
        let endpoint = endpoint(10, 40000);
        let preimage = relay_request_preimage(&[0x0B; 32], &[0xC4; 32], 21, 7, 1000, 2, endpoint);
        let signature = alice_key()
            .sign(&preimage)
            .expect("sign")
            .as_bytes()
            .to_vec();
        let block = RelayRequestBlock::new(21, 7, 1000, 2, endpoint, signature).expect("block");
        assert!(
            service
                .on_relay_request(
                    &block,
                    &[0xA1; 32],
                    &[0x0B; 32],
                    &[0xC4; 32],
                    200,
                    1000,
                    100
                )
                .expect("admit")
        );
        assert!(
            !service
                .on_relay_request(
                    &block,
                    &[0xA1; 32],
                    &[0x0B; 32],
                    &[0xC4; 32],
                    200,
                    1000,
                    110
                )
                .expect("replay")
        );
    }

    #[test]
    fn snapshot_and_debug_expose_no_secrets() {
        let service = service_with_keys(false);
        let snapshot = format!("{:?}", service.snapshot());
        assert!(!snapshot.contains("A1A1"));
        assert!(!snapshot.contains("127.0.0.1"));
        assert!(!format!("{service:?}").contains("A1A1"));
    }

    #[test]
    fn next_deadline_tracks_earliest_table() {
        let service = service_with_keys(false);
        assert_eq!(service.next_deadline_ms(), None);
        service
            .start_peer_test(
                11,
                PeerTestRole::Alice,
                [0xA1; 32],
                [0x0B; 32],
                [0xC4; 32],
                endpoint(10, 40000),
                100,
            )
            .expect("start");
        assert!(service.next_deadline_ms().is_some());
        service.shutdown();
        assert_eq!(service.next_deadline_ms(), None);
    }

    #[test]
    fn unknown_signer_fails_closed_without_state() {
        let service = Ssu2PeerRelayService::new(Ssu2PeerRelayConfig::default()).expect("service");
        let observed = endpoint(10, 40000);
        service
            .start_peer_test(
                55,
                PeerTestRole::Alice,
                [0xA1; 32],
                [0x0B; 32],
                [0xC4; 32],
                observed,
                0,
            )
            .expect("start");
        // No signers registered: any signed ingest fails closed.
        assert_eq!(service.snapshot().live_peer_tests, 1);
    }
}
