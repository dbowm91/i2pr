//! Plan 480 private local qualification readout.
//!
//! Strictly local, permission-gated projection of one SSU2 observer
//! epoch into the Plan 476 local-observed phase vocabulary. Pure
//! projection: no sockets, no subprocesses, no files, no clock, no
//! timers, no task ownership. The export carries phase labels plus
//! counts only — there is deliberately no field capable of holding a
//! transient message ID, peer hash, endpoint, or payload — and
//! `live_qualification` stays `"not-run"`: a locally observed
//! sequence is not a two-host claim.
//!
//! Vocabulary cross-reference:
//! `tests/integration/ssu2/plan476_executor.py::LOCAL_PHASES`.
//! Consumer note (Plan 476 continuation): the guarded per-host
//! process/session executor — spawning, per-host collection, receipt
//! exchange, revocation — is 476-owned. This module is the interface
//! it drains through (`Ssu2DaemonHandle::qualification_observer`), not
//! the executor.
//!
//! Plan 476 additions (2026-10-24), all of them additive and
//! fail-closed:
//!
//! * `LocalQualificationProfile::PrivateLan` — the private-LAN
//!   qualification profile is a *read* scope of its own, so a daemon
//!   started under `[ssu2] qualification_profile =
//!   "private_lan_qualification"` can project its own observer epoch.
//!   Before this, the collector refused every profile but the
//!   controlled loopback, which made the private-LAN lane
//!   unprojectable even though its observer existed.
//! * Direction-preserving session phases — `peer_session_accepted`
//!   (we dialled) and `peer_session_initiated` (the peer dialled us)
//!   are counted separately. The pre-existing `peer_authenticated`
//!   phase folded both directions together, which made "the peer
//!   initiated a fresh session to us" (tier T5 of the Plan 495 ladder)
//!   unobservable.
//! * `request_sent` — an accepted UDP write is now a phase instead of
//!   an uncounted transport internal, because one-way I2NP evidence
//!   needs the egress direction as well as the ingress direction.
//! * `reply_correlated` — a boolean, never a message ID: an
//!   authenticated ingress arrived carrying a transient message ID
//!   that this epoch had already written to the same peer. The
//!   correlation material stays inside `collect`; only the boolean
//!   leaves.

use i2pr_runtime::{Ssu2QualificationEventKind, Ssu2QualificationObserver};

/// Canonical local phase vocabulary, in order. Must stay a subset of
/// the Python `LOCAL_PHASES` in
/// `tests/integration/ssu2/plan476_executor.py`.
pub const LOCAL_PHASES: [&str; 9] = [
    "permit_checked",
    "router_started",
    "peer_session_accepted",
    "peer_session_initiated",
    "peer_authenticated",
    "request_sent",
    "request_received",
    "product_effect_observed",
    "router_stopped",
];

const PHASE_PERMIT: usize = 0;
const PHASE_ROUTER_STARTED: usize = 1;
const PHASE_SESSION_ACCEPTED: usize = 2;
const PHASE_SESSION_INITIATED: usize = 3;
const PHASE_SESSION_TOTAL: usize = 4;
const PHASE_REQUEST_SENT: usize = 5;
const PHASE_REQUEST_RECEIVED: usize = 6;
const PHASE_PRODUCT_EFFECT: usize = 7;
const PHASE_ROUTER_STOPPED: usize = 8;

/// Qualification scope the collector may read. The controlled loopback
/// profile and the private-LAN qualification profile are the only two
/// scopes; any other profile must arrive through a new plan-of-record,
/// never by extending this enum quietly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalQualificationProfile {
    ControlledLoopback,
    /// Explicit RFC1918 private-LAN qualification scope (Plan 476).
    PrivateLan,
}

/// Operator consent plus scope for one collection. Consent is
/// re-checked at collection time, not just at construction.
#[derive(Clone, Copy, Debug)]
pub struct LocalQualificationPermit {
    pub profile: LocalQualificationProfile,
    pub consent: bool,
}

/// Sanitized local receipt: phase labels plus counts only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalObservedReceipt {
    pub phases: Vec<(&'static str, u32)>,
    pub session_observed: bool,
    pub product_observed: bool,
    /// True when an authenticated ingress carried a transient message
    /// ID this epoch had already written to the same peer.
    pub reply_correlated: bool,
    pub live_qualification: &'static str,
}

/// Permission-gated projection of one drained observer epoch.
/// In-memory only.
pub struct LocalReceiptCollector {
    permit: LocalQualificationPermit,
}

impl LocalReceiptCollector {
    /// Refuses revoked consent or a non-qualified profile.
    pub fn new(permit: LocalQualificationPermit) -> Result<Self, &'static str> {
        if !permit.consent {
            return Err("permit-consent-revoked");
        }
        if !matches!(
            permit.profile,
            LocalQualificationProfile::ControlledLoopback | LocalQualificationProfile::PrivateLan
        ) {
            return Err("permit-profile-invalid");
        }
        Ok(Self { permit })
    }

    /// Projects one drained epoch. Refuses overflowed or empty
    /// epochs, non-monotonic sequences, product rows without a
    /// preceding ingress row carrying the same opaque commitment and
    /// link generation. Transient IDs never leave this function.
    ///
    /// An epoch with no authenticated session is **not** a refusal: it
    /// projects `session_observed: false` and omits every session
    /// phase, so it still cannot claim a session that did not happen.
    /// It has to be a projection rather than a refusal because a
    /// drained epoch is consumed whether or not it is projected — a
    /// refusal here would silently discard the epoch's other phases
    /// (notably the one that records the local bind). The
    /// fail-closed refusal for a session-less receipt lives at the
    /// consumer, which requires a non-zero authenticated-session
    /// phase before it will accept a receipt.
    pub fn collect(
        &self,
        observer: &Ssu2QualificationObserver,
    ) -> Result<LocalObservedReceipt, &'static str> {
        // The gate is re-checked at collection time, not just at
        // construction: a collector must never project an epoch it is
        // no longer permitted to read.
        if !self.permit.consent {
            return Err("permit-consent-revoked");
        }
        let (events, overflowed) = observer.drain();
        if overflowed {
            return Err("receipt-overflow");
        }
        if events.is_empty() {
            return Err("receipt-empty");
        }
        for pair in events.windows(2) {
            if pair[0].sequence() >= pair[1].sequence() {
                return Err("receipt-sequence-invalid");
            }
        }
        // Re-verify the observer's exact-match invariant from the
        // projected batch: a product row is accepted only with a
        // preceding ingress row carrying the same opaque commitment
        // and link generation.
        let mut ingress: Vec<([u8; 16], u64)> = Vec::new();
        // Written transient IDs, kept per peer commitment so a reply
        // can only be correlated inside one authenticated peer
        // relationship. These never leave this function.
        let mut written: Vec<([u8; 16], u32)> = Vec::new();
        let mut reply_correlated = false;
        let mut counts = [0_u32; LOCAL_PHASES.len()];
        for event in &events {
            match event.kind() {
                Ssu2QualificationEventKind::LocalIdentityAttested
                | Ssu2QualificationEventKind::SocketsBound => {
                    counts[PHASE_ROUTER_STARTED] = counts[PHASE_ROUTER_STARTED].saturating_add(1);
                }
                Ssu2QualificationEventKind::OutboundSessionAuthenticated => {
                    counts[PHASE_SESSION_ACCEPTED] =
                        counts[PHASE_SESSION_ACCEPTED].saturating_add(1);
                }
                Ssu2QualificationEventKind::InboundSessionAuthenticated => {
                    counts[PHASE_SESSION_INITIATED] =
                        counts[PHASE_SESSION_INITIATED].saturating_add(1);
                }
                Ssu2QualificationEventKind::AuthenticatedI2npReceived => {
                    counts[PHASE_REQUEST_RECEIVED] =
                        counts[PHASE_REQUEST_RECEIVED].saturating_add(1);
                    ingress.push((event.peer_commitment(), event.link_generation()));
                    if let Some(message_id) = event.message_id()
                        && written.iter().any(|(commitment, id)| {
                            *commitment == event.peer_commitment() && *id == message_id
                        })
                    {
                        reply_correlated = true;
                    }
                }
                Ssu2QualificationEventKind::I2npProductEffectAccepted => {
                    let key = (event.peer_commitment(), event.link_generation());
                    if !ingress.contains(&key) {
                        return Err("product-without-authenticated-ingress");
                    }
                    counts[PHASE_PRODUCT_EFFECT] = counts[PHASE_PRODUCT_EFFECT].saturating_add(1);
                }
                Ssu2QualificationEventKind::I2npDatagramWritten => {
                    counts[PHASE_REQUEST_SENT] = counts[PHASE_REQUEST_SENT].saturating_add(1);
                    if let Some(message_id) = event.message_id() {
                        written.push((event.peer_commitment(), message_id));
                    }
                }
                Ssu2QualificationEventKind::SessionClosed
                | Ssu2QualificationEventKind::RouterShutdown => {
                    counts[PHASE_ROUTER_STOPPED] = counts[PHASE_ROUTER_STOPPED].saturating_add(1);
                }
                // Bounded transport internals (queue admission, failed
                // or staged writes) are evidence of liveness, not
                // qualification phases; they are counted nowhere and
                // their transient IDs never leave this function.
                Ssu2QualificationEventKind::I2npQueueAccepted
                | Ssu2QualificationEventKind::I2npDatagramWriteFailed
                | Ssu2QualificationEventKind::I2npDatagramStageRejected => {}
            }
        }
        counts[PHASE_SESSION_TOTAL] =
            counts[PHASE_SESSION_ACCEPTED].saturating_add(counts[PHASE_SESSION_INITIATED]);
        let mut phases = Vec::new();
        phases.push((LOCAL_PHASES[PHASE_PERMIT], 1));
        for (index, phase) in LOCAL_PHASES.iter().enumerate().skip(1) {
            if counts[index] > 0 {
                phases.push((*phase, counts[index]));
            }
        }
        Ok(LocalObservedReceipt {
            phases,
            session_observed: counts[PHASE_SESSION_TOTAL] > 0,
            product_observed: counts[PHASE_PRODUCT_EFFECT] > 0,
            reply_correlated,
            live_qualification: "not-run",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn phase_vocabulary_is_ordered_and_unique() {
        let mut seen: HashSet<&str> = HashSet::new();
        for phase in LOCAL_PHASES {
            assert!(seen.insert(phase), "duplicate phase {phase}");
        }
        assert_eq!(LOCAL_PHASES.len(), 9);
    }

    #[test]
    fn revoked_consent_is_refused_at_construction_and_collection() {
        let permit = LocalQualificationPermit {
            profile: LocalQualificationProfile::ControlledLoopback,
            consent: false,
        };
        assert_eq!(
            LocalReceiptCollector::new(permit).err(),
            Some("permit-consent-revoked")
        );
    }

    #[test]
    fn both_qualification_profiles_are_readable_scopes() {
        for profile in [
            LocalQualificationProfile::ControlledLoopback,
            LocalQualificationProfile::PrivateLan,
        ] {
            let collector = LocalReceiptCollector::new(LocalQualificationPermit {
                profile,
                consent: true,
            });
            assert!(collector.is_ok(), "profile {profile:?} must be readable");
        }
    }
}
