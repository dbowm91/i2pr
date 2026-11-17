//! Bounded I2NP peer testing (Plan 285), and this router's **own** peer test
//! (Plan 521).
//!
//! A peer-testing message carries a message identifier and a timestamp. The
//! responder echoes both unchanged; the originator matches the echoed
//! identifier against an outstanding probe and derives round-trip time from
//! the timestamp it supplied.
//!
//! Two properties matter more than the codec here:
//!
//! - an inbound test that a peer expects an answer to gets a bounded, honest
//!   answer derived from the authenticated peer and link that delivered it, so
//!   an answer is never routed from a caller-supplied identity; and
//! - a probe this router originated is either matched or reported as timed
//!   out. The outstanding table is capacity-bounded and age-bounded, so an
//!   unanswered probe is never silently indistinguishable from a broken one
//!   and no table entry can be retained forever.
//!
//! The timestamp is an opaque probe value supplied by the originator. This
//! module never reads it as a local clock, so it cannot derive a round-trip
//! time by comparing it against local time. Round-trip time is measured by the
//! originator from the wall clock it recorded when it sent the probe.
//!
//! **Plan 521.** The module was complete and unwired: the responder's echo,
//! the bounded table and the dispatcher hook all existed, and none of them
//! had a production caller. This plan gives the module an **owner** — the
//! bounded [`PeerTestOwner`] that originates the router's own probes and
//! reports a **matched** echo — and it moves the responder onto the live
//! inbound path's **classified** outcome, so the inbound message is decoded
//! once and never twice.
//!
//! Two rules the owner holds to:
//!
//! - an echo completes a probe only when the probed identifier **and** the
//!   probed timestamp match an outstanding probe **and** the answer came from
//!   the peer the probe was sent to. A different peer cannot complete a probe,
//!   and a peer that answers with somebody else's identifier is refused rather
//!   than believed; and
//! - only a **matched** completion mints the transport's sealed
//!   [`ConfirmedPeerTest`] proof. A timeout, an unmatched echo and a peer
//!   mismatch record nothing, so a failed test can never poison later
//!   evidence; and
//! - a key this router has already answered is not answered again. The
//!   answered-key table is a ceiling like the probe table, so the reply
//!   traffic a peer can provoke is bounded by something the router controls
//!   and not by the peer's patience.

use std::collections::BTreeMap;

use i2pr_proto::{Date, I2npBody, I2npMessage, TunnelTestMessage};
use i2pr_transport::{ConfirmedPeerTest, LinkId, PeerId};
use rand_core::RngCore;

use crate::router_i2np::RouterI2npOutcome;

/// The bounded number of outstanding peer tests this router retains.
///
/// The table is a ceiling, not a target. A probe is refused rather than
/// evicting an older outstanding probe, so a peer's answer can never be
/// matched against a different probe than the one it answers.
pub const MAX_OUTSTANDING_PEER_TESTS: usize = 32;

/// The default age after which an outstanding probe is reported as timed out.
pub const DEFAULT_PEER_TEST_TIMEOUT_MS: u64 = 10_000;

/// The number of fresh identifiers a probe draws before it gives up.
///
/// The search is bounded: a router that cannot draw a fresh identifier in this
/// many tries refuses the probe rather than looping.
pub const MAX_PROBE_IDENTIFIER_TRIES: usize = 32;

/// The bounded number of answered peer tests this router retains.
///
/// The table remembers the keys this router has already answered, so one key
/// is answered at most once per age window. It is a ceiling, not a target: a
/// repeated key is refused rather than evicting an older answer.
pub const MAX_RESPONDED_PEER_TESTS: usize = 64;

/// Why a peer-test outcome was produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerTestOutcome {
    /// The message completed its round trip and the elapsed local time is
    /// reported. This is a local measurement, never a value derived from the
    /// probe's own timestamp.
    Completed {
        /// Local milliseconds between sending the probe and matching the echo.
        elapsed_ms: u64,
    },
    /// No outstanding probe matched the echoed identifier.
    Unmatched,
    /// The identifier matched an outstanding probe, but the answer came from a
    /// different peer than the probe was sent to. The probe is **not**
    /// completed and the outstanding entry is **not** released.
    PeerMismatch,
    /// The table is at its ceiling and the probe was refused. The router does
    /// not answer a probe it could not record.
    Refused,
    /// The probe was outstanding but has aged past the timeout.
    TimedOut {
        /// Local milliseconds the probe was outstanding.
        elapsed_ms: u64,
    },
}

/// The key an echoed peer test is matched against.
///
/// The probed identifier and the probed timestamp travel together, and the
/// responder echoes both unchanged. Matching on the pair — rather than on the
/// identifier alone — is what keeps one peer's probe from being mistaken for
/// the echo of this router's own probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct PeerTestKey {
    /// The probed message identifier the responder echoes unchanged.
    pub msg_id: u32,
    /// The opaque timestamp the responder echoes unchanged.
    pub timestamp: u64,
}

impl PeerTestKey {
    /// Builds a key.
    pub const fn new(msg_id: u32, timestamp: u64) -> Self {
        Self { msg_id, timestamp }
    }
}

/// One outstanding probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutstandingProbe {
    /// The local wall clock at send time. It ages the entry and nothing else.
    pub sent_ms: u64,
    /// The peer the probe was sent to.
    pub peer: PeerId,
}

/// The answer a responder owes an inbound peer test.
///
/// The peer and link travel from the authenticated inbound delivery, so the
/// answer is sent back on the same session that carried the test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerTestEcho {
    /// Authenticated peer that sent the test.
    pub peer: PeerId,
    /// Exact link the test arrived on.
    pub link_id: LinkId,
    /// Identifier to echo unchanged.
    pub msg_id: u32,
    /// Timestamp to echo unchanged.
    pub timestamp: u64,
}

impl PeerTestEcho {
    /// Builds the exact encoded echo body for this answer.
    ///
    /// The echo is a fresh message with a fresh transport message
    /// identifier, but it repeats the probed identifier and timestamp
    /// unchanged, which is what the originator matches.
    pub fn encode(
        &self,
        message_id: u32,
        expiration_ms: u64,
    ) -> Result<Vec<u8>, i2pr_proto::CodecError> {
        I2npMessage::new_standard(
            message_id,
            Date::from_millis(expiration_ms),
            I2npBody::TunnelTest(TunnelTestMessage {
                msg_id: self.msg_id,
                timestamp: self.timestamp,
            }),
        )?
        .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
    }
}

/// The answer built for one inbound peer test.
///
/// The answer travels back on the session that delivered the test, so the
/// peer and the link are carried alongside the encoded bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerTestAnswer {
    /// The authenticated peer that sent the test.
    pub peer: PeerId,
    /// The exact link the test arrived on.
    pub link_id: LinkId,
    /// The encoded answer.
    pub bytes: Vec<u8>,
}

/// Builds the answer owed by a **classified** dispatcher outcome.
///
/// The probed identifier and the probed timestamp are taken from the
/// dispatcher's outcome, which already carries them, so the live inbound path
/// decodes the inbound message **once** and never a second time.
///
/// Returns `None` when the outcome is not a peer test, so the responder can
/// never be turned into an echo oracle for another message type. A codec
/// failure is reported, never swallowed.
pub fn build_peer_test_answer(
    outcome: &RouterI2npOutcome,
    message_id: u32,
    expiration_ms: u64,
) -> Result<Option<PeerTestAnswer>, i2pr_proto::CodecError> {
    let RouterI2npOutcome::PeerTest {
        peer,
        link_id,
        peer_test_msg_id,
        peer_test_timestamp,
        ..
    } = outcome
    else {
        return Ok(None);
    };
    let echo = PeerTestEcho {
        peer: *peer,
        link_id: *link_id,
        msg_id: *peer_test_msg_id,
        timestamp: *peer_test_timestamp,
    };
    let bytes = echo.encode(message_id, expiration_ms)?;
    Ok(Some(PeerTestAnswer {
        peer: *peer,
        link_id: *link_id,
        bytes,
    }))
}

/// Builds the answer owed by a **classified** dispatcher outcome.
///
/// Returns `None` when the outcome is not a peer test, so the responder can
/// never be turned into an echo oracle for another message type.
pub fn answer_peer_test(
    outcome: &RouterI2npOutcome,
    message_id: u32,
    expiration_ms: u64,
) -> Option<PeerTestAnswer> {
    build_peer_test_answer(outcome, message_id, expiration_ms)
        .ok()
        .flatten()
}

/// What one inbound peer test turned out to be.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PeerTestInbound {
    /// The inbound test was the echo of a probe this router originated.
    ///
    /// The round trip is confirmed, and **no answer is owed**: answering an
    /// echo would open a second exchange with the peer that just answered.
    Completed(ConfirmedRoundTrip),
    /// The inbound test is owed an answer.
    ///
    /// The answer travels back to the peer that sent the test and repeats the
    /// probed identifier and timestamp unchanged.
    Answer(PeerTestAnswer),
    /// The inbound test owes nothing and changed nothing.
    Ignored,
}

/// Errors raised by [`PeerTestTracker`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PeerTestError {
    /// The table already holds the maximum number of outstanding probes.
    #[error("outstanding peer-test table is full")]
    Full,
    /// The requested age ceiling was zero, which would make every probe
    /// time out immediately.
    #[error("peer-test timeout must be nonzero")]
    InvalidTimeout,
}

/// A bounded table of outstanding peer tests.
///
/// The table is keyed by the probed identifier **and** the probed timestamp,
/// so an echo matches exactly the probe that produced it. Entries are removed
/// on match, on explicit expiry, and on drop; nothing is retained past its
/// deadline.
#[derive(Debug)]
pub struct PeerTestTracker {
    outstanding: BTreeMap<PeerTestKey, OutstandingProbe>,
    timeout_ms: u64,
}

impl PeerTestTracker {
    /// Creates a tracker with an explicit timeout ceiling.
    pub fn new(timeout_ms: u64) -> Result<Self, PeerTestError> {
        if timeout_ms == 0 {
            return Err(PeerTestError::InvalidTimeout);
        }
        Ok(Self {
            outstanding: BTreeMap::new(),
            timeout_ms,
        })
    }

    /// Creates a tracker with [`DEFAULT_PEER_TEST_TIMEOUT_MS`].
    pub fn with_default_timeout() -> Self {
        Self {
            outstanding: BTreeMap::new(),
            timeout_ms: DEFAULT_PEER_TEST_TIMEOUT_MS,
        }
    }

    /// Records a probe this router originated.
    ///
    /// `now_ms` is the local wall clock at send time and is used only to age
    /// the entry; the probed timestamp is not derived from it.
    pub fn record(
        &mut self,
        key: PeerTestKey,
        peer: PeerId,
        now_ms: u64,
    ) -> Result<(), PeerTestError> {
        if self.outstanding.len() >= MAX_OUTSTANDING_PEER_TESTS {
            return Err(PeerTestError::Full);
        }
        self.outstanding.insert(
            key,
            OutstandingProbe {
                sent_ms: now_ms,
                peer,
            },
        );
        Ok(())
    }

    /// Matches an echoed peer test against the outstanding table.
    ///
    /// Returns the local elapsed time on a match. A probe that has already
    /// aged out is reported as [`PeerTestOutcome::TimedOut`] and removed
    /// rather than reported as completed, so a late answer is not presented
    /// as a healthy round trip. An answer from a peer the probe was **not**
    /// sent to is reported as [`PeerTestOutcome::PeerMismatch`] and neither
    /// completes the probe nor releases its entry.
    pub fn complete(&mut self, key: PeerTestKey, peer: PeerId, now_ms: u64) -> PeerTestOutcome {
        let Some(probe) = self.outstanding.get(&key).copied() else {
            return PeerTestOutcome::Unmatched;
        };
        if probe.peer != peer {
            return PeerTestOutcome::PeerMismatch;
        }
        let elapsed_ms = now_ms.saturating_sub(probe.sent_ms);
        self.outstanding.remove(&key);
        if elapsed_ms > self.timeout_ms {
            return PeerTestOutcome::TimedOut { elapsed_ms };
        }
        PeerTestOutcome::Completed { elapsed_ms }
    }

    /// Removes one outstanding probe and reports whether it was present.
    ///
    /// The owner uses this to undo a probe it could not encode, so a failed
    /// probe never occupies a slot in the table.
    pub fn remove(&mut self, key: PeerTestKey) -> bool {
        self.outstanding.remove(&key).is_some()
    }

    /// Reports whether the peer already has an outstanding probe.
    ///
    /// One probe at a time per peer: a second attempt for the same peer is
    /// refused rather than queued, so the table never holds two probes whose
    /// answers could be confused with each other.
    pub fn has_outstanding_for(&self, peer: PeerId) -> bool {
        self.outstanding.values().any(|probe| probe.peer == peer)
    }

    /// Returns the number of outstanding probes.
    pub fn outstanding(&self) -> usize {
        self.outstanding.len()
    }

    /// Returns the configured age ceiling.
    pub const fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Drops every probe that has aged past the ceiling and returns the
    /// number removed, so a caller can observe expiry rather than infer it.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let timeout_ms = self.timeout_ms;
        let before = self.outstanding.len();
        self.outstanding
            .retain(|_, probe| now_ms.saturating_sub(probe.sent_ms) <= timeout_ms);
        before - self.outstanding.len()
    }
}

/// Why the owner refused to start a probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PeerTestOwnerError {
    /// The operator did not enable this router's own peer test.
    #[error("the router's own peer test is disabled")]
    Disabled,
    /// The outstanding-probe table is at its ceiling.
    #[error("the outstanding peer-test table is full")]
    Full,
    /// No fresh identifier could be drawn in the bounded number of tries.
    #[error("no fresh peer-test identifier could be drawn")]
    NoFreshIdentifier,
    /// The probe could not be encoded.
    #[error("the peer-test probe could not be encoded")]
    EncodeFailed,
    /// The peer already has an outstanding probe.
    ///
    /// One probe at a time per peer: a second attempt for the same peer is
    /// refused rather than queued.
    #[error("the peer already has an outstanding peer test")]
    InFlight,
}

/// One probe this router originated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerTestProbe {
    /// The peer the probe is for.
    pub peer: PeerId,
    /// The key the responder's echo is matched against.
    pub key: PeerTestKey,
    /// The encoded I2NP message to send.
    pub bytes: Vec<u8>,
}

/// A completed round trip with a peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfirmedRoundTrip {
    /// The peer that answered.
    pub peer: PeerId,
    /// The exact link the answer arrived on.
    ///
    /// The link names the session the exchange travelled on, so the runtime
    /// derives the address family from the session itself. The daemon never
    /// asserts a family.
    pub link_id: LinkId,
    /// The measured local round-trip time.
    pub elapsed_ms: u64,
    /// The transport's sealed proof of the matched round trip.
    pub proof: ConfirmedPeerTest,
}

/// The owner's counters, one per outcome.
///
/// Every refusal and every outcome is counted under its own reason, so a
/// caller can observe what happened instead of inferring it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeerTestOwnerCounters {
    /// Probes started and recorded.
    pub probes_started: u64,
    /// Probes refused because the operator left the peer test disabled.
    pub probes_refused_disabled: u64,
    /// Probes refused because the outstanding table was full.
    pub probes_refused_full: u64,
    /// Probes refused because no fresh identifier could be drawn.
    pub probes_refused_no_identifier: u64,
    /// Probes refused because the probe could not be encoded.
    pub probes_refused_encode: u64,
    /// Echoes that matched an outstanding probe.
    pub probes_completed: u64,
    /// Echoes that matched nothing.
    pub echoes_unmatched: u64,
    /// Echoes that matched an outstanding probe but came from another peer.
    pub echoes_peer_mismatch: u64,
    /// Probes that aged out before an answer arrived.
    pub probes_timed_out: u64,
    /// Probes dropped by an explicit expiry sweep.
    pub probes_expired: u64,
    /// Probes refused because the peer already had an outstanding probe.
    pub probes_refused_in_flight: u64,
    /// Answers built for an inbound peer test.
    pub answers_built: u64,
    /// Answers suppressed because the same key was already answered.
    pub answers_suppressed_repeat: u64,
    /// Answers refused because the answered-key table was at its ceiling.
    pub answers_refused_full: u64,
    /// Answers refused because the answer could not be encoded.
    pub answers_refused_encode: u64,
}

/// The bounded owner of this router's own peer tests.
///
/// The owner originates probes, tracks them in the bounded
/// [`PeerTestTracker`], and reports a **matched** echo as a confirmed round
/// trip. It holds no task of its own and starts none: the caller drives it
/// from an existing loop.
///
/// The owner sends nothing by itself. It produces the encoded probe and the
/// caller delivers it, so the delivery path, the session and the transport's
/// own budgets all stay where they are.
#[derive(Debug)]
pub struct PeerTestOwner {
    tracker: PeerTestTracker,
    /// The keys this router has already answered, with the local time of the
    /// answer.
    ///
    /// The table is a ceiling, not a target: a repeated key is refused rather
    /// than evicting an older answer, so the reply traffic a peer can provoke
    /// is bounded by this table and not by the peer's patience.
    responded: BTreeMap<PeerTestKey, u64>,
    counters: PeerTestOwnerCounters,
    enabled: bool,
}

impl PeerTestOwner {
    /// Creates an owner.
    ///
    /// `enabled` is the operator's opt-in. A disabled owner refuses every
    /// probe, so a router that has not asked for reachability evidence never
    /// generates probe traffic.
    pub fn new(enabled: bool) -> Self {
        Self {
            tracker: PeerTestTracker::with_default_timeout(),
            responded: BTreeMap::new(),
            counters: PeerTestOwnerCounters::default(),
            enabled,
        }
    }

    /// Whether the operator enabled this router's own peer test.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Starts one probe and returns the encoded message to send.
    ///
    /// The probe is recorded in the outstanding table **before** it is
    /// returned, so an answer that arrives quickly is never dropped. The
    /// probed identifier, the probed timestamp and the transport message
    /// identifier are drawn fresh, and the search is bounded.
    pub fn start_probe<R: RngCore>(
        &mut self,
        peer: PeerId,
        rng: &mut R,
        now_ms: u64,
        expiration_ms: u64,
    ) -> Result<PeerTestProbe, PeerTestOwnerError> {
        if !self.enabled {
            self.counters.probes_refused_disabled += 1;
            return Err(PeerTestOwnerError::Disabled);
        }
        if self.tracker.has_outstanding_for(peer) {
            self.counters.probes_refused_in_flight += 1;
            return Err(PeerTestOwnerError::InFlight);
        }
        let (message_id, key) = match fresh_probe_identifiers(rng) {
            Some(identifiers) => identifiers,
            None => {
                self.counters.probes_refused_no_identifier += 1;
                return Err(PeerTestOwnerError::NoFreshIdentifier);
            }
        };
        if let Err(PeerTestError::Full) = self.tracker.record(key, peer, now_ms) {
            self.counters.probes_refused_full += 1;
            return Err(PeerTestOwnerError::Full);
        }
        // The probe carries the same body the responder echoes. It is built
        // directly rather than through `PeerTestEcho`, because the
        // originator has no link: the link belongs to the responder's
        // authenticated delivery.
        let body = I2npBody::TunnelTest(TunnelTestMessage {
            msg_id: key.msg_id,
            timestamp: key.timestamp,
        });
        let bytes =
            match I2npMessage::new_standard(message_id, Date::from_millis(expiration_ms), body) {
                Ok(message) => match message
                    .encode_standard_to_vec(crate::router_i2np::MAX_ROUTER_I2NP_BYTES)
                {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        self.tracker.remove(key);
                        self.counters.probes_refused_encode += 1;
                        return Err(PeerTestOwnerError::EncodeFailed);
                    }
                },
                Err(_) => {
                    self.tracker.remove(key);
                    self.counters.probes_refused_encode += 1;
                    return Err(PeerTestOwnerError::EncodeFailed);
                }
            };
        self.counters.probes_started += 1;
        Ok(PeerTestProbe { peer, key, bytes })
    }

    /// Completes a probe from an inbound echo.
    ///
    /// Returns the confirmed round trip **only** when the echoed identifier
    /// and timestamp matched an outstanding probe and the answer came from the
    /// peer the probe was sent to. Every other outcome records nothing.
    pub fn complete_probe(
        &mut self,
        key: PeerTestKey,
        peer: PeerId,
        link_id: LinkId,
        now_ms: u64,
    ) -> Option<ConfirmedRoundTrip> {
        match self.match_inbound_probe(key, peer, now_ms) {
            PeerTestOutcome::Completed { elapsed_ms } => Some(ConfirmedRoundTrip {
                peer,
                link_id,
                elapsed_ms,
                proof: ConfirmedPeerTest::attest_matched_round_trip(elapsed_ms),
            }),
            _ => None,
        }
    }

    /// Matches one inbound echo against the outstanding table and counts the
    /// outcome.
    ///
    /// The match and its counters live in one place, so the completion path
    /// and the inbound handler can never disagree about what an echo meant.
    fn match_inbound_probe(
        &mut self,
        key: PeerTestKey,
        peer: PeerId,
        now_ms: u64,
    ) -> PeerTestOutcome {
        let outcome = self.tracker.complete(key, peer, now_ms);
        match outcome {
            PeerTestOutcome::Completed { .. } => self.counters.probes_completed += 1,
            PeerTestOutcome::Unmatched => self.counters.echoes_unmatched += 1,
            PeerTestOutcome::PeerMismatch => self.counters.echoes_peer_mismatch += 1,
            PeerTestOutcome::TimedOut { .. } => self.counters.probes_timed_out += 1,
            PeerTestOutcome::Refused => self.counters.probes_refused_full += 1,
        }
        outcome
    }

    /// Handles one inbound peer test on the live inbound path.
    ///
    /// The two roles are decided by the same match, so the inbound message is
    /// decoded once and the router never answers its own echo:
    ///
    /// - an echo that matches an outstanding probe **and** came from the peer
    ///   the probe was sent to completes the probe and is **not** answered; and
    /// - anything else is owed an answer, and the answer repeats the probed
    ///   identifier and timestamp unchanged.
    ///
    /// A late answer to one of this router's own probes is neither completed
    /// nor answered: the peer that sent it is not waiting for an answer, and
    /// answering would open a fresh exchange.
    pub fn handle_inbound(
        &mut self,
        outcome: &RouterI2npOutcome,
        message_id: u32,
        expiration_ms: u64,
        now_ms: u64,
    ) -> PeerTestInbound {
        let RouterI2npOutcome::PeerTest {
            peer,
            link_id,
            peer_test_msg_id,
            peer_test_timestamp,
            ..
        } = outcome
        else {
            return PeerTestInbound::Ignored;
        };
        let key = PeerTestKey::new(*peer_test_msg_id, *peer_test_timestamp);
        match self.match_inbound_probe(key, *peer, now_ms) {
            PeerTestOutcome::Completed { elapsed_ms } => {
                return PeerTestInbound::Completed(ConfirmedRoundTrip {
                    peer: *peer,
                    link_id: *link_id,
                    elapsed_ms,
                    proof: ConfirmedPeerTest::attest_matched_round_trip(elapsed_ms),
                });
            }
            PeerTestOutcome::TimedOut { .. } => {
                // The answer to one of this router's own probes arrived too
                // late. The peer that sent it is not waiting for an answer.
                return PeerTestInbound::Ignored;
            }
            PeerTestOutcome::Refused => {
                return PeerTestInbound::Ignored;
            }
            PeerTestOutcome::Unmatched | PeerTestOutcome::PeerMismatch => {}
        }
        // The responder's turn. A key this router has already answered is not
        // answered again, and the answered-key table is a ceiling: when it is
        // full the answer is refused rather than evicting an older one.
        if self.responded.contains_key(&key) {
            self.counters.answers_suppressed_repeat += 1;
            return PeerTestInbound::Ignored;
        }
        if self.responded.len() >= MAX_RESPONDED_PEER_TESTS {
            self.counters.answers_refused_full += 1;
            return PeerTestInbound::Ignored;
        }
        let answer = match build_peer_test_answer(outcome, message_id, expiration_ms) {
            Ok(Some(answer)) => answer,
            Ok(None) => return PeerTestInbound::Ignored,
            Err(_) => {
                self.counters.answers_refused_encode += 1;
                return PeerTestInbound::Ignored;
            }
        };
        self.responded.insert(key, now_ms);
        self.counters.answers_built += 1;
        PeerTestInbound::Answer(answer)
    }

    /// Expires the probes that aged out and returns the number dropped.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let expired = self.tracker.expire(now_ms);
        self.counters.probes_expired += expired as u64;
        let timeout_ms = self.tracker.timeout_ms();
        self.responded
            .retain(|_, answered_ms| now_ms.saturating_sub(*answered_ms) <= timeout_ms);
        expired
    }

    /// The number of answered keys this router currently remembers.
    pub fn responded(&self) -> usize {
        self.responded.len()
    }

    /// The number of outstanding probes.
    pub fn outstanding(&self) -> usize {
        self.tracker.outstanding()
    }

    /// The owner's counters.
    pub const fn counters(&self) -> &PeerTestOwnerCounters {
        &self.counters
    }
}

/// Draws a fresh, non-zero transport message identifier.
///
/// The transport message identifier of an answer must never be zero, and the
/// search is bounded: a router that cannot draw one in this many tries
/// refuses the answer rather than sending a message with an invalid
/// identifier.
pub fn fresh_message_identifier<R: RngCore>(rng: &mut R) -> Option<u32> {
    for _ in 0..MAX_PROBE_IDENTIFIER_TRIES {
        let message_id = rng.next_u32();
        if message_id != 0 {
            return Some(message_id);
        }
    }
    None
}

/// Draws a fresh transport message identifier and a fresh probe key.
///
/// The transport identifier is never zero. The probed identifier is never
/// zero either, and the probed timestamp is an opaque probe value: it is
/// drawn at random rather than taken from the clock, so it cannot be guessed
/// by a peer that wants its probe mistaken for this router's own.
fn fresh_probe_identifiers<R: RngCore>(rng: &mut R) -> Option<(u32, PeerTestKey)> {
    let mut message_id = 0u32;
    let mut msg_id = 0u32;
    let mut timestamp = 0u64;
    for _ in 0..MAX_PROBE_IDENTIFIER_TRIES {
        if message_id == 0 {
            message_id = rng.next_u32();
        }
        if msg_id == 0 {
            msg_id = rng.next_u32();
        }
        if timestamp == 0 {
            timestamp = rng.next_u64();
        }
        if message_id != 0 && msg_id != 0 && timestamp != 0 {
            return Some((message_id, PeerTestKey::new(msg_id, timestamp)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::{Hash, I2npMessage};
    use i2pr_transport::PeerId;
    use rand_core::SeedableRng;

    const NOW_MS: u64 = 1_700_000_000_000;

    fn peer(byte: u8) -> PeerId {
        PeerId::from_hash(Hash::from_bytes([byte; 32]))
    }

    /// The classified dispatcher outcome for an inbound peer test.
    fn classified_peer_test(msg_id: u32, timestamp: u64) -> RouterI2npOutcome {
        RouterI2npOutcome::PeerTest {
            header: crate::router_i2np::RouterI2npHeaderKind::Standard,
            message_id: 0x1234_5678,
            expiration_ms: NOW_MS + 30_000,
            peer_test_msg_id: msg_id,
            peer_test_timestamp: timestamp,
            peer: peer(0x11),
            link_id: LinkId::new(7).expect("link"),
            encoded_len: 32,
        }
    }

    /// The answer owed to an inbound peer test repeats the probed fields
    /// unchanged, so the originator can match it.
    #[test]
    fn the_answer_repeats_the_probed_fields_unchanged() {
        let answer = answer_peer_test(
            &classified_peer_test(0xABCD, 42),
            0x2222_3333,
            NOW_MS + 1_000,
        )
        .expect("answer owed");
        let decoded =
            I2npMessage::decode_standard(&answer.bytes, crate::router_i2np::MAX_ROUTER_I2NP_BYTES)
                .expect("decode");
        assert_eq!(
            decoded.body(),
            &I2npBody::TunnelTest(TunnelTestMessage {
                msg_id: 0xABCD,
                timestamp: 42
            })
        );
        assert_eq!(answer.peer, peer(0x11));
    }

    /// A non-peer-test outcome owes no answer, so the responder cannot be
    /// turned into an echo oracle for other message types.
    #[test]
    fn a_non_peer_test_outcome_owes_no_answer() {
        let outcome = RouterI2npOutcome::Unsupported {
            type_byte: 19,
            peer: peer(0x11),
            link_id: LinkId::new(7).expect("link"),
            encoded_len: 12,
        };
        assert_eq!(answer_peer_test(&outcome, 1, NOW_MS), None);
    }

    /// A probe that is answered within the ceiling reports the locally
    /// measured round trip, not a duration derived from the probe's own
    /// timestamp.
    #[test]
    fn a_completed_probe_reports_local_elapsed_time() {
        let mut tracker = PeerTestTracker::new(5_000).expect("tracker");
        let key = PeerTestKey::new(11, 77);
        tracker.record(key, peer(0x22), NOW_MS).expect("record");
        assert_eq!(
            tracker.complete(key, peer(0x22), NOW_MS + 250),
            PeerTestOutcome::Completed { elapsed_ms: 250 }
        );
        assert_eq!(tracker.outstanding(), 0);
    }

    /// The table is a hard ceiling and refuses rather than evicting, so an
    /// answer can never be matched against a different probe.
    #[test]
    fn the_table_refuses_at_ceiling_and_does_not_evict() {
        let mut tracker = PeerTestTracker::new(5_000).expect("tracker");
        for index in 0..MAX_OUTSTANDING_PEER_TESTS {
            tracker
                .record(PeerTestKey::new(index as u32, 1), peer(0x22), NOW_MS)
                .expect("record below ceiling");
        }
        assert_eq!(tracker.outstanding(), MAX_OUTSTANDING_PEER_TESTS);
        assert_eq!(
            tracker.record(PeerTestKey::new(9_999, 1), peer(0x22), NOW_MS),
            Err(PeerTestError::Full)
        );
        assert_eq!(
            tracker.complete(PeerTestKey::new(9_999, 1), peer(0x22), NOW_MS),
            PeerTestOutcome::Unmatched
        );
        assert_eq!(
            tracker.complete(PeerTestKey::new(0, 1), peer(0x22), NOW_MS),
            PeerTestOutcome::Completed { elapsed_ms: 0 }
        );
    }

    /// A late answer is reported as timed out rather than as a healthy round
    /// trip, so an unanswered probe is never silently healthy.
    #[test]
    fn a_late_answer_is_reported_as_timed_out() {
        let mut tracker = PeerTestTracker::new(1_000).expect("tracker");
        let key = PeerTestKey::new(3, 1);
        tracker.record(key, peer(0x22), NOW_MS).expect("record");
        assert_eq!(
            tracker.complete(key, peer(0x22), NOW_MS + 5_000),
            PeerTestOutcome::TimedOut { elapsed_ms: 5_000 }
        );
        assert_eq!(tracker.outstanding(), 0);
    }

    /// An echo with no matching probe is unmatched, not fabricated.
    #[test]
    fn an_unknown_identifier_is_unmatched() {
        let mut tracker = PeerTestTracker::with_default_timeout();
        assert_eq!(
            tracker.complete(PeerTestKey::new(4, 1), peer(0x22), NOW_MS),
            PeerTestOutcome::Unmatched
        );
    }

    /// An answer from a peer the probe was not sent to never completes the
    /// probe, and the outstanding entry survives so the real peer can still
    /// answer.
    #[test]
    fn an_answer_from_another_peer_is_refused() {
        let mut tracker = PeerTestTracker::with_default_timeout();
        let key = PeerTestKey::new(5, 1);
        tracker.record(key, peer(0x22), NOW_MS).expect("record");
        assert_eq!(
            tracker.complete(key, peer(0x33), NOW_MS + 10),
            PeerTestOutcome::PeerMismatch
        );
        assert_eq!(tracker.outstanding(), 1);
        assert_eq!(
            tracker.complete(key, peer(0x22), NOW_MS + 20),
            PeerTestOutcome::Completed { elapsed_ms: 20 }
        );
    }

    /// Expiry releases entries and is observable, so nothing is retained past
    /// its deadline and a leaked entry cannot accumulate.
    #[test]
    fn expiry_releases_aged_entries_and_is_observable() {
        let mut tracker = PeerTestTracker::new(1_000).expect("tracker");
        tracker
            .record(PeerTestKey::new(1, 1), peer(0x22), NOW_MS)
            .expect("record");
        tracker
            .record(PeerTestKey::new(2, 1), peer(0x22), NOW_MS + 900)
            .expect("record");
        assert_eq!(tracker.expire(NOW_MS + 500), 0);
        assert_eq!(tracker.outstanding(), 2);
        assert_eq!(tracker.expire(NOW_MS + 2_000), 2);
        assert_eq!(tracker.outstanding(), 0);
    }

    /// A zero timeout would make every probe time out immediately, so it is
    /// rejected rather than accepted as a degenerate table.
    #[test]
    fn a_zero_timeout_is_rejected() {
        assert!(matches!(
            PeerTestTracker::new(0),
            Err(PeerTestError::InvalidTimeout)
        ));
        assert_eq!(
            PeerTestTracker::with_default_timeout().timeout_ms(),
            DEFAULT_PEER_TEST_TIMEOUT_MS
        );
    }

    /// A disabled owner refuses every probe, so a router that has not asked
    /// for reachability evidence never generates probe traffic.
    #[test]
    fn a_disabled_owner_refuses_every_probe() {
        let mut owner = PeerTestOwner::new(false);
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([0u8; 32]);
        assert_eq!(
            owner.start_probe(peer(0x22), &mut rng, NOW_MS, NOW_MS + 30_000),
            Err(PeerTestOwnerError::Disabled)
        );
        assert_eq!(owner.counters().probes_refused_disabled, 1);
        assert_eq!(owner.outstanding(), 0);
    }

    /// A probe this router originated is matched by its own echo and mints the
    /// transport's sealed proof.
    #[test]
    fn a_matched_echo_completes_the_probe_and_mints_the_proof() {
        let mut owner = PeerTestOwner::new(true);
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([7u8; 32]);
        let probe = owner
            .start_probe(peer(0x22), &mut rng, NOW_MS, NOW_MS + 30_000)
            .expect("probe");
        assert_eq!(owner.outstanding(), 1);
        let confirmed = owner
            .complete_probe(
                probe.key,
                peer(0x22),
                LinkId::new(7).expect("link"),
                NOW_MS + 40,
            )
            .expect("confirmed");
        assert_eq!(confirmed.peer, peer(0x22));
        assert_eq!(confirmed.link_id, LinkId::new(7).expect("link"));
        assert_eq!(confirmed.elapsed_ms, 40);
        assert_eq!(confirmed.proof.elapsed_ms(), 40);
        assert_eq!(owner.counters().probes_completed, 1);
        assert_eq!(owner.outstanding(), 0);
    }

    /// A timeout records nothing, so a failed test never mints a proof.
    #[test]
    fn a_timeout_records_nothing() {
        let mut owner = PeerTestOwner::new(true);
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([9u8; 32]);
        let probe = owner
            .start_probe(peer(0x22), &mut rng, NOW_MS, NOW_MS + 30_000)
            .expect("probe");
        let late = NOW_MS + DEFAULT_PEER_TEST_TIMEOUT_MS + 1;
        assert_eq!(
            owner.complete_probe(probe.key, peer(0x22), LinkId::new(7).expect("link"), late),
            None
        );
        assert_eq!(owner.counters().probes_completed, 0);
        assert_eq!(owner.counters().probes_timed_out, 1);
    }

    /// An answer from another peer never completes the probe.
    #[test]
    fn an_answer_from_another_peer_completes_nothing() {
        let mut owner = PeerTestOwner::new(true);
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([3u8; 32]);
        let probe = owner
            .start_probe(peer(0x22), &mut rng, NOW_MS, NOW_MS + 30_000)
            .expect("probe");
        assert_eq!(
            owner.complete_probe(
                probe.key,
                peer(0x33),
                LinkId::new(7).expect("link"),
                NOW_MS + 10
            ),
            None
        );
        assert_eq!(owner.counters().probes_completed, 0);
        assert_eq!(owner.counters().echoes_peer_mismatch, 1);
        assert_eq!(owner.outstanding(), 1);
    }

    /// One probe at a time per peer: a second attempt for the same peer is
    /// refused rather than queued.
    #[test]
    fn a_second_probe_to_the_same_peer_is_refused() {
        let mut owner = PeerTestOwner::new(true);
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([11u8; 32]);
        owner
            .start_probe(peer(0x22), &mut rng, NOW_MS, NOW_MS + 30_000)
            .expect("first probe");
        assert_eq!(
            owner.start_probe(peer(0x22), &mut rng, NOW_MS, NOW_MS + 30_000),
            Err(PeerTestOwnerError::InFlight)
        );
        assert_eq!(owner.counters().probes_refused_in_flight, 1);
        assert_eq!(owner.outstanding(), 1);
    }

    /// The inbound handler answers a probe this router never sent, and it
    /// records nothing about itself: the responder's echo is not evidence of
    /// the responder's own reachability.
    #[test]
    fn the_inbound_handler_answers_a_foreign_probe_and_records_nothing() {
        let mut owner = PeerTestOwner::new(true);
        let outcome = classified_peer_test(0xABCD, 42);
        let answer = match owner.handle_inbound(&outcome, 0x2222_3333, NOW_MS + 1_000, NOW_MS) {
            PeerTestInbound::Answer(answer) => answer,
            other => panic!("expected an answer, got {other:?}"),
        };
        assert_eq!(answer.peer, peer(0x11));
        assert_eq!(answer.link_id, LinkId::new(7).expect("link"));
        assert_eq!(owner.counters().answers_built, 1);
        assert_eq!(owner.counters().probes_completed, 0);
        assert_eq!(owner.outstanding(), 0);
    }

    /// The echo of this router's own probe completes the probe and is owed no
    /// answer, so the router never answers itself.
    #[test]
    fn the_inbound_handler_completes_our_own_probe_and_owes_no_answer() {
        let mut owner = PeerTestOwner::new(true);
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([13u8; 32]);
        let probe = owner
            .start_probe(peer(0x11), &mut rng, NOW_MS, NOW_MS + 30_000)
            .expect("probe");
        let outcome = classified_peer_test(probe.key.msg_id, probe.key.timestamp);
        let confirmed = match owner.handle_inbound(&outcome, 1, NOW_MS + 1_000, NOW_MS + 25) {
            PeerTestInbound::Completed(confirmed) => confirmed,
            other => panic!("expected a completion, got {other:?}"),
        };
        assert_eq!(confirmed.peer, peer(0x11));
        assert_eq!(confirmed.elapsed_ms, 25);
        assert_eq!(confirmed.proof.elapsed_ms(), 25);
        assert_eq!(owner.counters().probes_completed, 1);
        assert_eq!(owner.counters().answers_built, 0);
        assert_eq!(owner.outstanding(), 0);
    }

    /// A key this router has already answered is not answered again, so the
    /// reply traffic a peer can provoke is bounded by the router's own table.
    #[test]
    fn a_repeated_key_is_answered_at_most_once() {
        let mut owner = PeerTestOwner::new(true);
        let outcome = classified_peer_test(0xABCD, 42);
        assert!(matches!(
            owner.handle_inbound(&outcome, 1, NOW_MS + 1_000, NOW_MS),
            PeerTestInbound::Answer(_)
        ));
        assert!(matches!(
            owner.handle_inbound(&outcome, 2, NOW_MS + 1_000, NOW_MS + 1),
            PeerTestInbound::Ignored
        ));
        assert_eq!(owner.counters().answers_built, 1);
        assert_eq!(owner.counters().answers_suppressed_repeat, 1);
    }

    /// A late answer to this router's own probe is neither completed nor
    /// answered: the peer that sent it is not waiting for an answer.
    #[test]
    fn a_late_echo_is_neither_completed_nor_answered() {
        let mut owner = PeerTestOwner::new(true);
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([17u8; 32]);
        let probe = owner
            .start_probe(peer(0x11), &mut rng, NOW_MS, NOW_MS + 30_000)
            .expect("probe");
        let outcome = classified_peer_test(probe.key.msg_id, probe.key.timestamp);
        let late = NOW_MS + DEFAULT_PEER_TEST_TIMEOUT_MS + 1;
        assert_eq!(
            owner.handle_inbound(&outcome, 1, NOW_MS + 1_000, late),
            PeerTestInbound::Ignored
        );
        assert_eq!(owner.counters().probes_completed, 0);
        assert_eq!(owner.counters().answers_built, 0);
        assert_eq!(owner.counters().probes_timed_out, 1);
    }

    /// The answered-key table is a ceiling: it refuses rather than evicting.
    #[test]
    fn the_answered_key_table_refuses_at_its_ceiling() {
        let mut owner = PeerTestOwner::new(true);
        for index in 0..MAX_RESPONDED_PEER_TESTS {
            let outcome = classified_peer_test(index as u32 + 1, 42);
            assert!(matches!(
                owner.handle_inbound(&outcome, 1, NOW_MS + 1_000, NOW_MS),
                PeerTestInbound::Answer(_)
            ));
        }
        let outcome = classified_peer_test(0xFFFF, 42);
        assert_eq!(
            owner.handle_inbound(&outcome, 1, NOW_MS + 1_000, NOW_MS),
            PeerTestInbound::Ignored
        );
        assert_eq!(owner.counters().answers_refused_full, 1);
        assert_eq!(owner.responded(), MAX_RESPONDED_PEER_TESTS);
    }

    /// The answered-key table ages on the owner's own sweep, so a remembered
    /// key cannot be retained forever.
    #[test]
    fn the_answered_key_table_ages_out() {
        let mut owner = PeerTestOwner::new(true);
        let outcome = classified_peer_test(0xABCD, 42);
        assert!(matches!(
            owner.handle_inbound(&outcome, 1, NOW_MS + 1_000, NOW_MS),
            PeerTestInbound::Answer(_)
        ));
        assert_eq!(owner.responded(), 1);
        owner.expire(NOW_MS + DEFAULT_PEER_TEST_TIMEOUT_MS + 1);
        assert_eq!(owner.responded(), 0);
    }

    /// The table's ceiling is the owner's ceiling, and a refused probe is
    /// counted rather than evicting an older one.
    #[test]
    fn the_owner_refuses_at_the_table_ceiling() {
        let mut owner = PeerTestOwner::new(true);
        let mut rng = rand_chacha::ChaCha8Rng::from_seed([5u8; 32]);
        for index in 0..MAX_OUTSTANDING_PEER_TESTS {
            owner
                .start_probe(peer(index as u8), &mut rng, NOW_MS, NOW_MS + 30_000)
                .expect("probe below ceiling");
        }
        assert_eq!(
            owner.start_probe(peer(0xEE), &mut rng, NOW_MS, NOW_MS + 30_000),
            Err(PeerTestOwnerError::Full)
        );
        assert_eq!(owner.counters().probes_refused_full, 1);
        assert_eq!(owner.outstanding(), MAX_OUTSTANDING_PEER_TESTS);
    }
}
