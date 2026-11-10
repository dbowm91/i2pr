//! Plan 499 — the bounded drain from the runtime's relay handoff into the
//! SSU2 introducer (Bob) role owner, and the carriage that puts the role
//! owner's produced block on the wire.
//!
//! Plan 490 built the role owner and the runtime's relay handoff, and
//! recorded that the daemon's ordinary inbound pump never drained that
//! handoff: a Relay Request a peer really sent was decoded, authenticated
//! and then dropped. Plan 498 added the drain. This module owns both the
//! drain and the **carriage**: the produced Relay Intro or Relay Response
//! is put on the session with its destination peer instead of being
//! dropped.
//!
//! Properties:
//!
//! - The drain is **bounded**. It pulls until the source is empty or the
//!   per-pass bound is reached, whichever comes first. The runtime's relay
//!   channel is itself bounded (`max_inbound_relay_queue`, default 16).
//! - The drain **never fails** its caller. A refusal, a resolver miss and a
//!   block that cannot be carried are counted outcomes, not errors.
//! - The drain **never logs** a peer hash, a key, a tag or a payload. The
//!   counters carry counts only.
//! - The source is a **pull** seam. It cannot enqueue into the runtime's
//!   relay channel; that channel's sender is private to the runtime and is
//!   fed only by the authenticated transport path.
//! - The carriage happens **after** the role owner admitted and verified
//!   the block. The carriage cannot make an unverifiable request
//!   verifiable, and it cannot make the role owner admit a block it would
//!   otherwise refuse.

use std::collections::BTreeMap;

use i2pr_runtime::{
    RelayIntroBlock, RelayResponseBlock, Ssu2InboundRelay, Ssu2RelayEvent, Ssu2RelayQueueError,
    Ssu2RuntimeService,
};

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

/// The carriage that puts a produced relay block on the wire.
///
/// Plan 499. The role owner produces a block **for a peer**: a Relay Intro
/// for the introduction target, a Relay Response for the requester. The
/// block itself carries no destination — the session is the destination —
/// so the carriage is told which peer the block is for.
///
/// This is a **pull-shaped** seam beside [`RelayEventSource`]: it exists so
/// the drain's behaviour can be exercised without binding a socket. It
/// cannot enqueue into the runtime's relay channel, and it cannot weaken
/// the role owner's verification, because the carriage runs only after the
/// role owner admitted and verified the block.
pub trait RelayCarriage {
    /// Carries one Relay Intro to the introduction target.
    fn carry_intro(
        &self,
        target: &[u8; 32],
        block: &RelayIntroBlock,
    ) -> Result<(), Ssu2RelayQueueError>;

    /// Carries one Relay Response to the requester.
    fn carry_response(
        &self,
        requester: &[u8; 32],
        block: &RelayResponseBlock,
    ) -> Result<(), Ssu2RelayQueueError>;
}

/// The production carriage: the runtime that owns the sessions.
///
/// The runtime resolves the destination peer to its live session and puts
/// the block on that session's outbound control queue. The runtime owns
/// the session registry, so this type holds no session state of its own.
pub struct RuntimeRelayCarriage {
    runtime: Ssu2RuntimeService,
}

impl RuntimeRelayCarriage {
    /// Creates a carriage over the runtime that owns the live sessions.
    pub fn new(runtime: Ssu2RuntimeService) -> Self {
        Self { runtime }
    }
}

impl RelayCarriage for RuntimeRelayCarriage {
    fn carry_intro(
        &self,
        target: &[u8; 32],
        block: &RelayIntroBlock,
    ) -> Result<(), Ssu2RelayQueueError> {
        self.runtime
            .queue_relay_intro(i2pr_transport::PeerId::from_bytes(*target), block.clone())
    }

    fn carry_response(
        &self,
        requester: &[u8; 32],
        block: &RelayResponseBlock,
    ) -> Result<(), Ssu2RelayQueueError> {
        self.runtime.queue_relay_response(
            i2pr_transport::PeerId::from_bytes(*requester),
            block.clone(),
        )
    }
}

/// The source the requester drain pulls from.
///
/// Plan 531. It is a **pull** seam like [`RelayEventSource`]: it cannot
/// enqueue into the runtime's handoff, and it cannot bypass the
/// requester owner's verification, because the transport verifies every
/// event the owner hands to it.
pub trait RequesterEventSource {
    /// Returns the next pending requester event, or `None` when none is
    /// pending.
    fn try_next_requester_event(&mut self) -> Option<i2pr_runtime::Ssu2InboundRequesterEvent>;
}

/// The counted outcome of one requester drain pass.
///
/// Counts only: no peer hash, no key, no tag, no payload.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RequesterDrainOutcome {
    /// Requester events pulled from the source.
    pub drained: u64,
    /// Events the requester owner handled.
    pub handled: u64,
    /// Events the requester owner refused.
    pub refused: u64,
    /// Exchanges that completed and recorded the relay-firewalled fact.
    pub firewalled: u64,
    /// Hole punches the owner held until their accept arrived (Plan 533).
    ///
    /// A held punch is **handled**, not refused: the transport requires
    /// the accept before a hole punch, and the punch travels one hop
    /// while the accept travels two.
    pub deferred: u64,
}

/// Drains the runtime's requester handoff into the requester (Alice) role
/// owner.
///
/// Plan 531. The pass is bounded by [`MAX_RELAY_DRAIN_PER_PASS`]: it
/// pulls until the source is empty or the bound is reached, whichever
/// comes first. A refusal is a counted outcome, not an error -- the
/// drain never fails the caller's loop.
///
/// The target's signing key is resolved from the **validated** store by
/// the caller's resolver. A target this router does not hold yields
/// `None`, and the owner refuses the event rather than guessing.
pub fn drain_requester_events(
    source: &mut dyn RequesterEventSource,
    owner: &mut crate::relay_requester::RelayRequesterOwner,
    signers: &dyn crate::ssu2_introducer::IntroducerSignerResolver,
    now_ms: u64,
    now_secs: u64,
) -> RequesterDrainOutcome {
    let mut outcome = RequesterDrainOutcome::default();
    for _ in 0..MAX_RELAY_DRAIN_PER_PASS {
        let Some(event) = source.try_next_requester_event() else {
            break;
        };
        outcome.drained = outcome.drained.saturating_add(1);
        let result = match event {
            i2pr_runtime::Ssu2InboundRequesterEvent::Response(inbound) => {
                let i2pr_runtime::Ssu2RelayEvent::Response(block) = &inbound.event else {
                    outcome.refused = outcome.refused.saturating_add(1);
                    continue;
                };
                if owner.context_for(block.nonce()).is_none() {
                    outcome.refused = outcome.refused.saturating_add(1);
                    continue;
                }
                let accepted = owner.on_relay_response(
                    block,
                    *inbound.peer.hash().as_bytes(),
                    signers,
                    now_secs,
                    now_ms,
                );
                // Plan 533: the accept's correlation is what releases a
                // hole punch that arrived before it. The punch is
                // presented again through the transport's own seam, so
                // the transport's verification runs on it once more.
                if !matches!(
                    accepted,
                    crate::relay_requester::RelayRequesterOutcome::ResponseAccepted
                ) {
                    accepted
                } else if let Some(punched) =
                    owner.redrive_deferred(block.nonce(), signers, now_secs, now_ms)
                {
                    // Both events are handled: the accept correlated, and
                    // the punch it released completed the exchange.
                    outcome.handled = outcome.handled.saturating_add(1);
                    punched
                } else {
                    accepted
                }
            }
            i2pr_runtime::Ssu2InboundRequesterEvent::HolePunch(inbound) => {
                if owner
                    .context_for(inbound.message.response.nonce())
                    .is_none()
                {
                    outcome.refused = outcome.refused.saturating_add(1);
                    continue;
                }
                owner.on_hole_punch(&inbound.message, signers, now_secs, now_ms)
            }
        };
        match result {
            crate::relay_requester::RelayRequesterOutcome::Refused(_) => {
                outcome.refused = outcome.refused.saturating_add(1);
            }
            crate::relay_requester::RelayRequesterOutcome::HolePunchReady => {
                outcome.handled = outcome.handled.saturating_add(1);
                outcome.firewalled = outcome.firewalled.saturating_add(1);
            }
            crate::relay_requester::RelayRequesterOutcome::PunchDeferred => {
                outcome.handled = outcome.handled.saturating_add(1);
                outcome.deferred = outcome.deferred.saturating_add(1);
            }
            _ => {
                outcome.handled = outcome.handled.saturating_add(1);
            }
        }
    }
    outcome
}

/// The source the target drain pulls from.
///
/// Plan 532. It is a **pull** seam like [`RequesterEventSource`]: it cannot
/// enqueue into the runtime's handoff, and it cannot bypass the target
/// owner's verification, because the transport verifies every event the
/// owner hands to it.
pub trait TargetEventSource {
    /// Returns the next pending target event, or `None` when none is
    /// pending.
    fn try_next_target_event(&mut self) -> Option<i2pr_runtime::Ssu2InboundTarget>;
}

/// The counted outcome of one target drain pass.
///
/// Counts only: no peer hash, no key, no tag, no payload.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TargetDrainOutcome {
    /// Target events pulled from the source.
    pub drained: u64,
    /// Introductions the target owner answered.
    pub answered: u64,
    /// Introductions the target owner refused.
    pub refused: u64,
    /// Replayed introductions the transport suppressed.
    pub replays: u64,
}

/// Drains the runtime's target handoff into the target (Charlie) role
/// owner.
///
/// Plan 532. The pass is bounded by [`MAX_RELAY_DRAIN_PER_PASS`]: it pulls
/// until the source is empty or the bound is reached, whichever comes
/// first. A refusal is a counted outcome, not an error -- the drain never
/// fails the caller's loop.
///
/// The requester's validated record is read out of the **authoritative**
/// store the caller passes. A requester this router does not hold yields
/// a typed refusal, and the refusal writes **nothing** into the
/// requester's profile.
pub fn drain_target_events(
    source: &mut dyn TargetEventSource,
    owner: &mut crate::relay_target::RelayTargetOwner,
    store: &i2pr_netdb::RouterInfoStore,
    now_secs: u64,
    now_ms: u64,
) -> TargetDrainOutcome {
    let mut outcome = TargetDrainOutcome::default();
    for _ in 0..MAX_RELAY_DRAIN_PER_PASS {
        let Some(event) = source.try_next_target_event() else {
            break;
        };
        outcome.drained = outcome.drained.saturating_add(1);
        match crate::relay_target::drive_relay_target(owner, store, &event, now_secs, now_ms) {
            crate::relay_target::RelayTargetOutcome::Answered => {
                outcome.answered = outcome.answered.saturating_add(1);
            }
            crate::relay_target::RelayTargetOutcome::ReplaySuppressed => {
                outcome.replays = outcome.replays.saturating_add(1);
            }
            crate::relay_target::RelayTargetOutcome::Refused(_) => {
                outcome.refused = outcome.refused.saturating_add(1);
            }
        }
    }
    outcome
}

/// The counted outcome of one drain pass.
///
/// Counts only: no peer hash, no key, no tag, no payload.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RelayDrainOutcome {
    /// Relay events pulled from the source.
    pub drained: u64,
    /// Events the role owner admitted.
    pub admitted: u64,
    /// Events the role owner refused.
    pub refused: u64,
    /// Blocks the role owner produced **and** the carriage put on the wire.
    pub carried: u64,
    /// Blocks the role owner produced that could **not** be carried, by
    /// reason.
    ///
    /// A block that cannot be carried is counted here, never dropped
    /// without a trace. The keys are the runtime's stable reason labels
    /// (`Ssu2RelayQueueError::reason`); no peer identity appears in them.
    pub carriage_refusals: BTreeMap<&'static str, u64>,
    /// Relay-tag answers the drain pulled and did not hand to the role
    /// owner.
    ///
    /// Plan 524. A `Tag` is the answer to *this router's own* request for
    /// a relay tag. It is not an inbound request, the role owner never
    /// served it, and counting it as one of the role owner's refusals was
    /// wrong. The runtime records the answer itself; the drain only
    /// reports that it saw one.
    pub tag_answers: u64,
}

/// Drains the runtime's relay handoff into the introducer role owner.
///
/// Plan 498. The pass is bounded by [`MAX_RELAY_DRAIN_PER_PASS`]: it pulls
/// until the source is empty or the bound is reached, whichever comes first.
/// A refusal is a counted outcome, not an error — the drain never fails the
/// caller's loop.
///
/// Plan 499. A block the role owner produces is **carried**: the carriage
/// puts it on the session with the peer it is for. A carriage that cannot
/// be performed is counted by reason, and the drain continues.
pub fn drain_relay_events(
    source: &mut dyn RelayEventSource,
    owner: &mut Ssu2IntroducerService,
    signers: &dyn IntroducerSignerResolver,
    carriage: &dyn RelayCarriage,
    now_ms: u64,
    now_secs: u64,
) -> RelayDrainOutcome {
    let mut outcome = RelayDrainOutcome::default();
    for _ in 0..MAX_RELAY_DRAIN_PER_PASS {
        let Some(event) = source.try_next_relay_event() else {
            break;
        };
        outcome.drained = outcome.drained.saturating_add(1);
        // Plan 524: a relay-tag answer is not an inbound request. The
        // role owner never served it and must not be charged with a
        // refusal it did not make.
        if matches!(event.event, Ssu2RelayEvent::Tag(_)) {
            outcome.tag_answers = outcome.tag_answers.saturating_add(1);
            continue;
        }
        match owner.on_relay_event(&event, signers, now_ms, now_secs) {
            Ssu2IntroducerOutcome::Refused { .. } => {
                outcome.refused = outcome.refused.saturating_add(1);
            }
            // The role owner produced a block for a peer. The carriage puts
            // it on that peer's session. A carriage that cannot be performed
            // is counted by reason, never dropped silently.
            Ssu2IntroducerOutcome::IntroRequired { target, intro } => {
                outcome.admitted = outcome.admitted.saturating_add(1);
                match carriage.carry_intro(&target, &intro) {
                    Ok(()) => {
                        outcome.carried = outcome.carried.saturating_add(1);
                    }
                    Err(reason) => {
                        *outcome
                            .carriage_refusals
                            .entry(reason.reason())
                            .or_insert(0) += 1;
                    }
                }
            }
            Ssu2IntroducerOutcome::ResponseRequired {
                requester,
                response,
            } => {
                outcome.admitted = outcome.admitted.saturating_add(1);
                match carriage.carry_response(&requester, &response) {
                    Ok(()) => {
                        outcome.carried = outcome.carried.saturating_add(1);
                    }
                    Err(reason) => {
                        *outcome
                            .carriage_refusals
                            .entry(reason.reason())
                            .or_insert(0) += 1;
                    }
                }
            }
        }
    }
    outcome
}
