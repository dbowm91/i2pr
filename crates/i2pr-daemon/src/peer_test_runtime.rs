//! Plan 547 — this router's **own** peer test, owned beside the SSU2 service.
//!
//! Plan 521 gave the router an I2NP peer test and put its owner inside the
//! destination-group product. That was the only place the owner could live
//! while the product was the only component that owned an inbound path — but
//! the product is built only when a destination group can exist at all, so a
//! router that runs **only** SSU2 had no owner, sent no probe, and could
//! never corroborate its own reachability.
//!
//! Plan 546 relaxed the publication policy so that a bound address *may* be
//! published, and left the corroboration the policy needs with nowhere to
//! come from. The owner lives here now, with the transport it measures:
//!
//! - it is built from `ssu2.peer_test` **alone**, so it exists whenever the
//!   SSU2 service does and not only when a destination group can;
//! - it is driven by the daemon's **existing** loop — the same tick the
//!   destination-group product advances on — so it adds no task, no timer
//!   and no listener; and
//! - it has exactly **one** consumer, the daemon's own inbound path. Two
//!   owners would answer the same probe twice and double the counters, so
//!   the guard pins the single consumer.
//!
//! Three rules the owner keeps:
//!
//! - the **runtime** is the only writer of reachability. The daemon hands
//!   the runtime a matched round trip and the runtime derives the address
//!   family from the session the exchange travelled on, so the daemon never
//!   asserts one;
//! - a peer test is an **outcome**, not an error. A refused probe, a
//!   timeout and an unmatched echo are each counted under their own reason
//!   and none of them stops the router; and
//! - the responder records **nothing** about itself. Only a completed round
//!   trip records anything.

use std::sync::Arc;
use std::time::Duration;

use i2pr_transport::PeerId;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use crate::peer_test::{PeerTestInbound, PeerTestOwner, PeerTestOwnerCounters};
use crate::router_i2np::{
    RouterDeliveryOutcome, RouterDeliveryRequest, RouterI2npOutcome, Ssu2DaemonHandle,
};

/// The interval between two probes of this router's own peer test.
///
/// The probe is a **rare** event: the transport's own publication cadence
/// does not depend on it, and a router that reaches nobody is not a router
/// that is failing.
pub const PEER_TEST_INTERVAL_MS: u64 = 300_000;

/// The delay before a refused attempt is tried again.
///
/// A refused attempt is never queued: the probe is dropped and the next
/// attempt waits, so a router with no peer to ask generates no traffic.
pub const PEER_TEST_RETRY_MS: u64 = 30_000;

/// The lifetime of one probe: the window in which its answer may arrive.
pub const PEER_TEST_LIFETIME_MS: u64 = 60_000;

/// The bounded outbound deadline for one probe or one answer.
const PEER_TEST_DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// Plan 547 — the router's own peer test, owned beside the SSU2 service.
///
/// The runtime holds the bounded [`PeerTestOwner`], the entropy source the
/// owner's identifiers are drawn from, and the cadence of this router's own
/// probes. It holds **no** task, **no** timer and **no** listener: the
/// caller's tick drives it and the caller's inbound path feeds it.
#[derive(Debug)]
pub struct PeerTestRuntime {
    /// The bounded owner, present only when the operator enabled the peer
    /// test.
    ///
    /// A router that has not asked for reachability evidence has **no**
    /// owner at all, exactly as before Plan 547: it sends no probe, answers
    /// no probe, and its counters stay at zero.
    owner: Option<PeerTestOwner>,
    /// The entropy source the owner's fresh identifiers are drawn from.
    ///
    /// A router that cannot reach the operating system's entropy source has
    /// no source either, so it never sends a probe or an answer whose
    /// identifiers it could not make unique.
    rng: Option<ChaCha8Rng>,
    /// The instant at which the next probe may start.
    next_at_ms: u64,
    /// The live counters cell, published through the inspection seam.
    counters_cell: Arc<crate::i2pcontrol_inspection::PeerTestCounterCell>,
}

impl PeerTestRuntime {
    /// Builds the runtime from the operator's opt-in.
    ///
    /// `enabled` is `ssu2.peer_test`. The opt-in is **off** by default, so
    /// the default router owns no peer test at all.
    pub fn new(enabled: bool) -> Self {
        Self {
            owner: enabled.then(|| PeerTestOwner::new(true)),
            rng: if enabled {
                ChaCha8Rng::try_from_os_rng().ok()
            } else {
                None
            },
            next_at_ms: 0,
            counters_cell: Arc::new(std::sync::Mutex::new(PeerTestOwnerCounters::default())),
        }
    }

    /// Whether the operator enabled this router's own peer test.
    pub fn enabled(&self) -> bool {
        self.owner.as_ref().is_some_and(|owner| owner.enabled())
    }

    /// The live counters cell.
    ///
    /// The cell carries counts and stable reason labels only: no peer, no
    /// address, no key and no payload.
    pub fn counters_cell(&self) -> Arc<crate::i2pcontrol_inspection::PeerTestCounterCell> {
        Arc::clone(&self.counters_cell)
    }

    /// Handles one inbound peer test on the live inbound path.
    ///
    /// The seam reads the **classified** dispatcher outcome, so the inbound
    /// message is decoded exactly once and the responder runs no decoder of
    /// its own. The owner's match decides the role:
    ///
    /// - an echo of this router's own probe completes the probe and is owed
    ///   **no** answer, so the router never answers itself; and
    /// - anything else is owed an answer that repeats the probed identifier
    ///   and timestamp unchanged.
    ///
    /// The responder records **nothing** about itself. Only a completed round
    /// trip records anything, and it records the router's **own**
    /// reachability through the runtime's own recorder.
    pub fn accept_inbound(
        &mut self,
        outcome: &RouterI2npOutcome,
        handle: &Ssu2DaemonHandle,
        token: &i2pr_runtime::CancellationToken,
        now_ms: u64,
    ) {
        let action = {
            let Some(owner) = self.owner.as_mut() else {
                return;
            };
            let Some(rng) = self.rng.as_mut() else {
                return;
            };
            let Some(message_id) = crate::peer_test::fresh_message_identifier(rng) else {
                return;
            };
            owner.handle_inbound(
                outcome,
                message_id,
                now_ms.saturating_add(PEER_TEST_LIFETIME_MS),
                now_ms,
            )
        };
        match action {
            PeerTestInbound::Completed(round_trip) => {
                // The runtime derives the address family from the session
                // the exchange travelled on, so the daemon never asserts
                // one.
                let service = handle.service();
                let elapsed_ms = round_trip.proof.elapsed_ms();
                if let Err(error) =
                    service.note_confirmed_peer_test(round_trip.link_id, round_trip.proof)
                {
                    tracing::debug!(
                        transition = "peer_test_evidence_refused",
                        reason = ?error,
                        "the router's own confirmed peer test was not recorded"
                    );
                } else {
                    // Plan 547: the round trip is the router's own reachability
                    // evidence, so it is reported. The report carries the
                    // **measured** round-trip time and nothing else: no peer,
                    // no address, no key and no payload.
                    tracing::info!(
                        transition = "peer_test_round_trip_completed",
                        elapsed_millis = elapsed_ms,
                        "the router's own peer test completed a matched round trip"
                    );
                }
            }
            PeerTestInbound::Answer(answer) => {
                self.deliver_peer_test_answer(answer, handle, token);
            }
            PeerTestInbound::Ignored => {}
        }
        self.publish_counters();
    }

    /// Delivers one peer-test answer to the peer that asked for it.
    ///
    /// The answer travels to the **authenticated** peer the inbound test
    /// arrived from, over the router's own delivery seam. A peer with no
    /// established session reports `NoActiveSession` explicitly and the
    /// refusal is counted by the transport.
    fn deliver_peer_test_answer(
        &self,
        answer: crate::peer_test::PeerTestAnswer,
        handle: &Ssu2DaemonHandle,
        token: &i2pr_runtime::CancellationToken,
    ) {
        let request =
            match RouterDeliveryRequest::new(answer.peer, answer.bytes, PEER_TEST_DELIVERY_TIMEOUT)
            {
                Ok(request) => request,
                Err(_) => return,
            };
        let outcome = handle.delivery().deliver(request, token);
        if !matches!(outcome, RouterDeliveryOutcome::Accepted) {
            tracing::debug!(
                transition = "peer_test_answer_refused",
                delivery_outcome = ?outcome,
                "a peer-test answer was refused by the transport"
            );
        } else {
            // Plan 547: the answer is the **responder's** half of the round
            // trip, and a router that answers nobody can never corroborate
            // anybody's reachability. The report carries counts only.
            tracing::info!(
                transition = "peer_test_answer_sent",
                "the router answered a peer's peer test"
            );
        }
    }

    /// Drives this router's own peer test.
    ///
    /// The owner holds no task, no timer and no listener: the caller's tick
    /// starts one probe on a bounded cadence and expires the probes that
    /// aged out, and the live inbound path answers the peer's test. A router
    /// whose operator has not enabled the peer test does nothing here, and a
    /// router with no peer to ask refuses the attempt rather than queueing
    /// it.
    pub fn drive(
        &mut self,
        handle: &Ssu2DaemonHandle,
        token: &i2pr_runtime::CancellationToken,
        now_ms: u64,
    ) {
        let enabled = self
            .owner
            .as_ref()
            .map(|owner| owner.enabled())
            .unwrap_or(false);
        if !enabled {
            return;
        }
        if let Some(owner) = self.owner.as_mut() {
            owner.expire(now_ms);
        }
        if now_ms < self.next_at_ms {
            return;
        }
        let Some(peer) = self.select_peer_test_target(handle) else {
            // No peer with a session: the attempt is refused and counted by
            // the retry delay, never queued.
            //
            // Plan 547: the refusal is **reported**. A peer test that never
            // leaves the router and a peer test that is never started read
            // exactly the same in a log that says nothing, and the two have
            // opposite causes.
            tracing::info!(
                transition = "peer_test_no_peer_to_ask",
                "the router's own peer test has no peer with a live session"
            );
            self.next_at_ms = now_ms.saturating_add(PEER_TEST_RETRY_MS);
            return;
        };
        let started = {
            let Some(owner) = self.owner.as_mut() else {
                return;
            };
            let Some(rng) = self.rng.as_mut() else {
                return;
            };
            owner.start_probe(
                peer,
                rng,
                now_ms,
                now_ms.saturating_add(PEER_TEST_LIFETIME_MS),
            )
        };
        let probe = match started {
            Ok(probe) => probe,
            Err(error) => {
                tracing::info!(
                    transition = "peer_test_probe_refused",
                    reason = %error,
                    "the router's own peer test refused a probe"
                );
                self.next_at_ms = now_ms.saturating_add(PEER_TEST_RETRY_MS);
                self.publish_counters();
                return;
            }
        };
        self.next_at_ms = now_ms.saturating_add(PEER_TEST_INTERVAL_MS);
        let request =
            match RouterDeliveryRequest::new(probe.peer, probe.bytes, PEER_TEST_DELIVERY_TIMEOUT) {
                Ok(request) => request,
                Err(_) => return,
            };
        // Plan 547: the probe is **reported**. The report names no peer and
        // no address: it says that a probe left this router and that the
        // transport accepted it, which is the difference between "the peer
        // test never ran" and "the peer never answered".
        tracing::info!(
            transition = "peer_test_probe_started",
            "the router's own peer test sent a probe"
        );
        let outcome = handle.delivery().deliver(request, token);
        if !matches!(outcome, RouterDeliveryOutcome::Accepted) {
            tracing::info!(
                transition = "peer_test_probe_refused",
                delivery_outcome = ?outcome,
                "the router's own peer test probe was refused by the transport"
            );
        }
        self.publish_counters();
    }

    /// Selects one peer with a live session for this router's own peer test.
    ///
    /// The selection asks the runtime for the peers it holds a session with
    /// and takes the first in a stable order. It never dials, never builds a
    /// pool and never names a peer the router has no session with, so a probe
    /// always travels over an established session.
    fn select_peer_test_target(&self, handle: &Ssu2DaemonHandle) -> Option<PeerId> {
        let mut hashes = handle.service().active_peer_hashes();
        hashes.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        hashes.first().map(|hash| PeerId::from_hash(*hash))
    }

    /// Publishes the owner's counters into the live cell.
    ///
    /// The cell carries counts only. A poisoned lock is a gap, never a
    /// zero: the counters keep the last published value.
    fn publish_counters(&self) {
        let Some(owner) = self.owner.as_ref() else {
            return;
        };
        if let Ok(mut cell) = self.counters_cell.lock() {
            *cell = *owner.counters();
        }
    }
}
