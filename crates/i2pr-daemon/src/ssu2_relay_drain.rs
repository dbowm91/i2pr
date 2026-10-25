//! Plan 498 — the bounded drain from the runtime's relay handoff into the
//! SSU2 introducer (Bob) role owner.
//!
//! Plan 490 built the role owner and the runtime's relay handoff, and
//! recorded that the daemon's ordinary inbound pump never drained the
//! handoff: a Relay Request that a peer really sent was decoded,
//! authenticated and then dropped. This module owns the drain.
//!
//! Properties:
//!
//! - The drain is **bounded**. It pulls until the source is empty or the
//!   per-pass bound is reached, whichever comes first. The runtime's relay
//!   channel is itself bounded (`max_inbound_relay_queue`, default 16).
//! - The drain **never fails** its caller. A refusal, a resolver miss and a
//!   produced-but-not-carried block are counted outcomes, not errors.
//! - The drain **never logs** a peer hash, a key, a tag or a payload. The
//!   counters carry counts only.
//! - The source is a **pull** seam. It cannot enqueue into the runtime's
//!   relay channel; that channel's sender is private to the runtime and is
//!   fed only by the authenticated transport path.
//!
//! What this module does **not** do is carry a produced Relay Intro or
//! Relay Response onto the wire. The runtime has no outbound relay queueing
//! API; that is Plan 490 Medium #2's own scope, and the
//! [`RelayDrainOutcome::awaiting_carriage`] counter is what keeps the gap
//! from being a silent drop.

use i2pr_runtime::Ssu2InboundRelay;

use crate::ssu2_introducer::{
    IntroducerSignerResolver, Ssu2IntroducerOutcome, Ssu2IntroducerService,
};

/// The maximum number of relay events one drain pass processes.
///
/// The runtime's relay channel is bounded by `max_inbound_relay_queue`
/// (default 16, ceiling `MAX_SSU2_INBOUND_QUEUE_CEILING`), so one pass can
/// never exceed the channel's capacity. The bound is restated here so it is
/// visible at the drain and not only in the runtime.
pub const MAX_RELAY_DRAIN_PER_PASS: usize = 16;

/// The source the drain pulls relay events from.
///
/// Plan 498: this is a **pull** seam. It cannot enqueue into the runtime's
/// relay channel — that channel's sender is private to the runtime and is
/// fed only by the authenticated transport path. The seam exists so the
/// drain's behaviour can be exercised without binding a socket, and it
/// cannot bypass the role owner's verification: an event still needs a
/// valid signature from a key the resolver supplies only for a peer whose
/// RouterInfo the NetDB already validated.
pub trait RelayEventSource {
    /// Returns the next pending relay event, or `None` when none is pending.
    fn try_next_relay_event(&mut self) -> Option<Ssu2InboundRelay>;
}

/// The counted outcome of one drain pass.
///
/// Counts only: no peer hash, no key, no tag, no payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelayDrainOutcome {
    /// Relay events pulled from the source.
    pub drained: u64,
    /// Events the role owner admitted.
    pub admitted: u64,
    /// Events the role owner refused.
    pub refused: u64,
    /// Blocks the role owner produced that must still be carried to a peer.
    ///
    /// Plan 498 does not carry them: the runtime has no outbound relay
    /// queueing API, which is Plan 490 Medium #2's own scope. The count is
    /// what keeps the block from being a silent drop.
    pub awaiting_carriage: u64,
}

/// Drains the runtime's relay handoff into the introducer role owner.
///
/// Plan 498. The pass is bounded by [`MAX_RELAY_DRAIN_PER_PASS`]: it pulls
/// until the source is empty or the bound is reached, whichever comes first.
/// A refusal is a counted outcome, not an error — the drain never fails the
/// caller's loop.
pub fn drain_relay_events(
    source: &mut dyn RelayEventSource,
    owner: &mut Ssu2IntroducerService,
    signers: &dyn IntroducerSignerResolver,
    now_ms: u64,
    now_secs: u64,
) -> RelayDrainOutcome {
    let mut outcome = RelayDrainOutcome::default();
    for _ in 0..MAX_RELAY_DRAIN_PER_PASS {
        let Some(event) = source.try_next_relay_event() else {
            break;
        };
        outcome.drained = outcome.drained.saturating_add(1);
        match owner.on_relay_event(&event, signers, now_ms, now_secs) {
            Ssu2IntroducerOutcome::Refused { .. } => {
                outcome.refused = outcome.refused.saturating_add(1);
            }
            // The role owner produced a block for a peer. Carrying it is
            // Plan 490 Medium #2's scope; the count is what keeps the
            // block from being a silent drop.
            Ssu2IntroducerOutcome::IntroRequired { .. }
            | Ssu2IntroducerOutcome::ResponseRequired { .. } => {
                outcome.admitted = outcome.admitted.saturating_add(1);
                outcome.awaiting_carriage = outcome.awaiting_carriage.saturating_add(1);
            }
        }
    }
    outcome
}
