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
//! exchange, revocation — is 476-owned and still open. This module is
//! the interface it drains through
//! (`Ssu2DaemonHandle::qualification_observer`), not the executor.

use i2pr_runtime::{Ssu2QualificationEventKind, Ssu2QualificationObserver};

/// Canonical local phase vocabulary, in order. Must stay a subset of
/// the Python `LOCAL_PHASES` in
/// `tests/integration/ssu2/plan476_executor.py`.
pub const LOCAL_PHASES: [&str; 6] = [
    "permit_checked",
    "router_started",
    "peer_authenticated",
    "request_received",
    "product_effect_observed",
    "router_stopped",
];

/// Qualification scope the collector may read. Only the controlled
/// loopback profile exists: any other profile must arrive through a
/// new plan-of-record, never by extending this enum quietly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalQualificationProfile {
    ControlledLoopback,
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
    pub live_qualification: &'static str,
}

/// Permission-gated projection of one drained observer epoch.
/// In-memory only.
pub struct LocalReceiptCollector {
    permit: LocalQualificationPermit,
}

impl LocalReceiptCollector {
    /// Refuses revoked consent or a non-controlled profile.
    pub fn new(permit: LocalQualificationPermit) -> Result<Self, &'static str> {
        if !permit.consent {
            return Err("permit-consent-revoked");
        }
        if permit.profile != LocalQualificationProfile::ControlledLoopback {
            return Err("permit-profile-invalid");
        }
        Ok(Self { permit })
    }

    /// Projects one drained epoch. Refuses overflowed or empty
    /// epochs, non-monotonic sequences, product rows without a
    /// preceding ingress row carrying the same opaque commitment and
    /// link generation, and epochs with no authenticated session.
    /// Transient IDs never leave this function.
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
        let mut counts = [0_u32; LOCAL_PHASES.len()];
        for event in &events {
            match event.kind() {
                Ssu2QualificationEventKind::LocalIdentityAttested
                | Ssu2QualificationEventKind::SocketsBound => {
                    counts[1] = counts[1].saturating_add(1);
                }
                Ssu2QualificationEventKind::OutboundSessionAuthenticated
                | Ssu2QualificationEventKind::InboundSessionAuthenticated => {
                    counts[2] = counts[2].saturating_add(1);
                }
                Ssu2QualificationEventKind::AuthenticatedI2npReceived => {
                    counts[3] = counts[3].saturating_add(1);
                    ingress.push((event.peer_commitment(), event.link_generation()));
                }
                Ssu2QualificationEventKind::I2npProductEffectAccepted => {
                    let key = (event.peer_commitment(), event.link_generation());
                    if !ingress.contains(&key) {
                        return Err("product-without-authenticated-ingress");
                    }
                    counts[4] = counts[4].saturating_add(1);
                }
                Ssu2QualificationEventKind::SessionClosed
                | Ssu2QualificationEventKind::RouterShutdown => {
                    counts[5] = counts[5].saturating_add(1);
                }
                // Bounded transport internals (queue admission, socket
                // write dispositions) are evidence of liveness, not
                // qualification phases; they are counted nowhere and
                // their transient IDs never leave this function.
                Ssu2QualificationEventKind::I2npQueueAccepted
                | Ssu2QualificationEventKind::I2npDatagramWritten
                | Ssu2QualificationEventKind::I2npDatagramWriteFailed
                | Ssu2QualificationEventKind::I2npDatagramStageRejected => {}
            }
        }
        if counts[2] == 0 {
            return Err("authenticated-session-unobserved");
        }
        let mut phases = Vec::new();
        phases.push((LOCAL_PHASES[0], 1));
        for (index, phase) in LOCAL_PHASES.iter().enumerate().skip(1) {
            if counts[index] > 0 {
                phases.push((*phase, counts[index]));
            }
        }
        Ok(LocalObservedReceipt {
            phases,
            session_observed: true,
            product_observed: counts[4] > 0,
            live_qualification: "not-run",
        })
    }
}
