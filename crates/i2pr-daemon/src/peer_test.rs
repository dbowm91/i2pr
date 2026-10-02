//! Bounded I2NP peer testing (Plan 285).
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

use std::collections::BTreeMap;

use i2pr_proto::{Date, I2npBody, I2npMessage, TunnelTestMessage};
use i2pr_runtime::Ssu2InboundI2np;
use i2pr_transport::{LinkId, PeerId};

use crate::router_i2np::{
    RouterI2npError, RouterI2npOutcome, dispatch_router_i2np_with_transit_bodies,
};

/// The bounded number of outstanding peer tests this router retains.
///
/// The table is a ceiling, not a target. A probe is refused rather than
/// evicting an older outstanding probe, so a peer's answer can never be
/// matched against a different probe than the one it answers.
pub const MAX_OUTSTANDING_PEER_TESTS: usize = 32;

/// The default age after which an outstanding probe is reported as timed out.
pub const DEFAULT_PEER_TEST_TIMEOUT_MS: u64 = 10_000;

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
    /// The table is at its ceiling and the probe was refused. The router does
    /// not answer a probe it could not record.
    Refused,
    /// The probe was outstanding but has aged past the timeout.
    TimedOut {
        /// Local milliseconds the probe was outstanding.
        elapsed_ms: u64,
    },
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
/// The table is keyed by the probed message identifier so an echo matches
/// exactly the probe that produced it. Entries are removed on match, on
/// explicit expiry, and on drop; nothing is retained past its deadline.
#[derive(Debug)]
pub struct PeerTestTracker {
    outstanding: BTreeMap<u32, u64>,
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
    pub fn record(&mut self, msg_id: u32, now_ms: u64) -> Result<(), PeerTestError> {
        if self.outstanding.len() >= MAX_OUTSTANDING_PEER_TESTS {
            return Err(PeerTestError::Full);
        }
        self.outstanding.insert(msg_id, now_ms);
        Ok(())
    }

    /// Matches an echoed identifier against the outstanding table.
    ///
    /// Returns the local elapsed time on a match. A probe that has already
    /// aged out is reported as [`PeerTestOutcome::TimedOut`] and removed
    /// rather than reported as completed, so a late answer is not presented
    /// as a healthy round trip.
    pub fn complete(&mut self, msg_id: u32, now_ms: u64) -> PeerTestOutcome {
        let Some(sent_ms) = self.outstanding.remove(&msg_id) else {
            return PeerTestOutcome::Unmatched;
        };
        let elapsed_ms = now_ms.saturating_sub(sent_ms);
        if elapsed_ms > self.timeout_ms {
            return PeerTestOutcome::TimedOut { elapsed_ms };
        }
        PeerTestOutcome::Completed { elapsed_ms }
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
            .retain(|_, sent_ms| now_ms.saturating_sub(*sent_ms) <= timeout_ms);
        before - self.outstanding.len()
    }
}

/// Extracts the peer-test answer owed by one authenticated inbound message.
///
/// Returns the ordinary dispatch outcome together with the answer to send,
/// or `None` when the message is not a peer test. A malformed or oversized
/// message still fails closed through the ordinary dispatcher.
pub fn dispatch_router_i2np_with_peer_test(
    inbound: &Ssu2InboundI2np,
    now_ms: u64,
) -> Result<(RouterI2npOutcome, Option<PeerTestEcho>), RouterI2npError> {
    let (outcome, _bodies) = dispatch_router_i2np_with_transit_bodies(inbound, now_ms)?;
    let echo = peer_test_echo(inbound)?;
    Ok((outcome, echo))
}

fn peer_test_echo(inbound: &Ssu2InboundI2np) -> Result<Option<PeerTestEcho>, RouterI2npError> {
    if inbound.bytes.is_empty() || inbound.bytes.len() > crate::router_i2np::MAX_ROUTER_I2NP_BYTES {
        return Ok(None);
    }
    let standard = I2npMessage::decode_standard(
        inbound.bytes.as_slice(),
        crate::router_i2np::MAX_ROUTER_I2NP_BYTES,
    );
    let message = match standard {
        Ok(message) => message,
        Err(_) => match I2npMessage::decode_short_transport(
            inbound.bytes.as_slice(),
            crate::router_i2np::MAX_ROUTER_I2NP_BYTES,
        ) {
            Ok(message) => message,
            Err(_) => return Ok(None),
        },
    };
    let I2npBody::TunnelTest(test) = message.body() else {
        return Ok(None);
    };
    Ok(Some(PeerTestEcho {
        peer: inbound.peer,
        link_id: inbound.link_id,
        msg_id: test.msg_id,
        timestamp: test.timestamp,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::{Hash, I2npMessage};
    use i2pr_transport::PeerId;

    const NOW_MS: u64 = 1_700_000_000_000;

    fn inbound_with(bytes: Vec<u8>) -> Ssu2InboundI2np {
        Ssu2InboundI2np {
            link_id: LinkId::new(7).expect("link"),
            peer: PeerId::from_hash(Hash::from_bytes([0x11; 32])),
            bytes,
        }
    }

    fn peer_test_message(msg_id: u32, timestamp: u64) -> Vec<u8> {
        I2npMessage::new_standard(
            0x1234_5678,
            Date::from_millis(NOW_MS + 30_000),
            I2npBody::TunnelTest(TunnelTestMessage { msg_id, timestamp }),
        )
        .expect("message")
        .encode_standard_to_vec(crate::router_i2np::MAX_ROUTER_I2NP_BYTES)
        .expect("encode")
    }

    /// A test that a peer expects answered produces an answer, and the answer
    /// repeats the probed identifier and timestamp unchanged so the originator
    /// can match it.
    #[test]
    fn inbound_peer_test_yields_echo_of_unchanged_fields() {
        let (outcome, echo) = dispatch_router_i2np_with_peer_test(
            &inbound_with(peer_test_message(0xABCD, 42)),
            NOW_MS,
        )
        .expect("dispatch");
        let echo = echo.expect("echo owed");
        assert_eq!(echo.msg_id, 0xABCD);
        assert_eq!(echo.timestamp, 42);
        assert_eq!(echo.link_id, LinkId::new(7).expect("link"));
        // The answer travels back over the authenticated session, never from a
        // caller-supplied identity.
        assert_eq!(echo.peer, PeerId::from_hash(Hash::from_bytes([0x11; 32])));
        assert!(matches!(outcome, RouterI2npOutcome::PeerTest { .. }));
    }

    /// The encoded echo is a fresh message that repeats the probed fields.
    #[test]
    fn echo_encodes_a_fresh_message_repeating_the_probe() {
        let echo = PeerTestEcho {
            peer: PeerId::from_hash(Hash::from_bytes([0x11; 32])),
            link_id: LinkId::new(7).expect("link"),
            msg_id: 0x1234_5678,
            timestamp: 99,
        };
        let encoded = echo.encode(0x2222_3333, NOW_MS + 1_000).expect("encode");
        let decoded =
            I2npMessage::decode_standard(&encoded, crate::router_i2np::MAX_ROUTER_I2NP_BYTES)
                .expect("decode");
        assert!(
            matches!(
                decoded.header(),
                i2pr_proto::I2npHeader::Standard { message_id, .. } if message_id == 0x2222_3333
            ),
            "echo is a fresh transport message"
        );
        assert_eq!(
            decoded.body(),
            &I2npBody::TunnelTest(TunnelTestMessage {
                msg_id: 0x1234_5678,
                timestamp: 99
            })
        );
    }

    /// A non-peer-test message owes no answer, so the responder cannot be
    /// turned into an echo oracle for other message types.
    #[test]
    fn non_peer_test_message_owes_no_echo() {
        let bytes = I2npMessage::new_standard(
            1,
            Date::from_millis(NOW_MS + 30_000),
            I2npBody::DeliveryStatus(i2pr_proto::DeliveryStatusMessage::new(
                7,
                Date::from_millis(NOW_MS),
            )),
        )
        .expect("message")
        .encode_standard_to_vec(crate::router_i2np::MAX_ROUTER_I2NP_BYTES)
        .expect("encode");
        let (_, echo) =
            dispatch_router_i2np_with_peer_test(&inbound_with(bytes), NOW_MS).expect("dispatch");
        assert_eq!(echo, None);
    }

    /// A probe that is answered within the ceiling reports the locally
    /// measured round trip, not a duration derived from the probe's own
    /// timestamp.
    #[test]
    fn completed_probe_reports_local_elapsed_time() {
        let mut tracker = PeerTestTracker::new(5_000).expect("tracker");
        tracker.record(11, NOW_MS).expect("record");
        assert_eq!(
            tracker.complete(11, NOW_MS + 250),
            PeerTestOutcome::Completed { elapsed_ms: 250 }
        );
        assert_eq!(tracker.outstanding(), 0);
    }

    /// The table is a hard ceiling and refuses rather than evicting, so an
    /// answer can never be matched against a different probe.
    #[test]
    fn table_refuses_at_ceiling_and_does_not_evict() {
        let mut tracker = PeerTestTracker::new(5_000).expect("tracker");
        for index in 0..MAX_OUTSTANDING_PEER_TESTS {
            tracker
                .record(index as u32, NOW_MS)
                .expect("record below ceiling");
        }
        assert_eq!(tracker.outstanding(), MAX_OUTSTANDING_PEER_TESTS);
        assert_eq!(tracker.record(9_999, NOW_MS), Err(PeerTestError::Full));
        // The refused probe is not present, and the original entries survive.
        assert_eq!(tracker.complete(9_999, NOW_MS), PeerTestOutcome::Unmatched);
        assert_eq!(
            tracker.complete(0, NOW_MS),
            PeerTestOutcome::Completed { elapsed_ms: 0 }
        );
    }

    /// A late answer is reported as timed out rather than as a healthy round
    /// trip, so an unanswered probe is never silently healthy.
    #[test]
    fn late_answer_is_reported_as_timed_out() {
        let mut tracker = PeerTestTracker::new(1_000).expect("tracker");
        tracker.record(3, NOW_MS).expect("record");
        assert_eq!(
            tracker.complete(3, NOW_MS + 5_000),
            PeerTestOutcome::TimedOut { elapsed_ms: 5_000 }
        );
        // The entry is released on the timed-out path too.
        assert_eq!(tracker.outstanding(), 0);
    }

    /// An echo with no matching probe is unmatched, not fabricated.
    #[test]
    fn unknown_identifier_is_unmatched() {
        let mut tracker = PeerTestTracker::with_default_timeout();
        assert_eq!(tracker.complete(4, NOW_MS), PeerTestOutcome::Unmatched);
    }

    /// Expiry releases entries and is observable, so nothing is retained past
    /// its deadline and a leaked entry cannot accumulate.
    #[test]
    fn expiry_releases_aged_entries_and_is_observable() {
        let mut tracker = PeerTestTracker::new(1_000).expect("tracker");
        tracker.record(1, NOW_MS).expect("record");
        tracker.record(2, NOW_MS + 900).expect("record");
        assert_eq!(tracker.expire(NOW_MS + 500), 0);
        assert_eq!(tracker.outstanding(), 2);
        assert_eq!(tracker.expire(NOW_MS + 2_000), 2);
        assert_eq!(tracker.outstanding(), 0);
    }

    /// A zero timeout would make every probe time out immediately, so it is
    /// rejected rather than accepted as a degenerate table.
    #[test]
    fn zero_timeout_is_rejected() {
        assert!(matches!(
            PeerTestTracker::new(0),
            Err(PeerTestError::InvalidTimeout)
        ));
        assert_eq!(
            PeerTestTracker::with_default_timeout().timeout_ms(),
            DEFAULT_PEER_TEST_TIMEOUT_MS
        );
    }
}
