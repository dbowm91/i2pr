//! Plan 503 — the peer-eligibility decision surface.
//!
//! Plan 491 attached two duties to the API version this router declares:
//! minimum peers a tunnel may be built through, and minimum floodfill peers a
//! DSM may be sent to. The decision has a receipt shape, and this module owns
//! that shape together with the live owner that produces it.
//!
//! The receipt carries **counts and stable reason labels only**. It never
//! carries a peer hash, an address, a key, a version string, or a payload.
//!
//! The owner is the daemon's live, bounded log of those receipts. It is
//! runtime-agnostic: no Tokio, no socket, no task, no channel. Its lock is
//! held only for the length of a push or a clone, and never across an
//! `.await`.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use i2pr_netdb::{VersionGate, VersionRefusalCounts};
use i2pr_proto::Hash;

use crate::destination_peers::CandidateProjectionSummary;

/// The maximum number of peer-eligibility receipts the live owner retains.
///
/// A **constant**, not a configuration key: the log is a bounded window of
/// recent decisions for an operator to read, not a tunable ledger. A
/// configuration key would let an operator grow the structure without bound,
/// or shrink it to the point where no decision is observable at all; the
/// ceiling is part of the receipt owner's contract instead.
pub const MAX_VERSION_ELIGIBILITY_RECEIPTS: usize = 64;

/// The peer-eligibility gate a version receipt describes.
///
/// Plan 491. The gate names the enforcement point; the floor it applies is
/// specification policy, not an operator setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionDecisionKind {
    /// The tunnel-build peer selection: the minimum peer version a tunnel
    /// may be built through.
    TunnelBuildPeer,
    /// The DSM publication target selection: the minimum floodfill version a
    /// DSM may be sent to.
    DsmTarget,
}

impl VersionDecisionKind {
    /// Returns the stable label of the gate.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::TunnelBuildPeer => "tunnel-build-peer",
            Self::DsmTarget => "dsm-target",
        }
    }

    /// The selection gate this decision applies.
    ///
    /// Crate-private: the gate is the policy's own wiring, not part of the
    /// receipt's public surface.
    pub(crate) const fn selection_gate(&self) -> VersionGate {
        match self {
            Self::TunnelBuildPeer => VersionGate::TunnelBuildPeer,
            Self::DsmTarget => VersionGate::DsmTarget,
        }
    }
}

/// A sanitized receipt of one peer-eligibility decision.
///
/// The receipt carries counts and stable reason labels only. It never carries
/// a peer hash, an address, a key, a version string, or a payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionEligibilityReceipt {
    /// The gate the decision applied.
    pub kind: VersionDecisionKind,
    /// The number of peers the decision examined.
    pub examined: u32,
    /// The number of peers the decision accepted.
    pub accepted: u32,
    /// The number of peers the decision refused, keyed by the stable reason
    /// label.
    pub refusals: BTreeMap<&'static str, u32>,
    /// A digest over the receipt's counts.
    pub digest: [u8; 32],
}

impl VersionEligibilityReceipt {
    /// The number of peers the decision refused.
    pub fn refused(&self) -> u32 {
        self.refusals.values().copied().sum()
    }

    /// The receipt for a tunnel-build peer decision.
    ///
    /// `examined` is the number of records the projection considered and
    /// `accepted` is the number of candidates that survived every refusal.
    /// The refusal map carries the three *version* reasons only — the
    /// projection's `unqualified` and `capped_participation` reasons are not
    /// version refusals and are counted in `CandidateProjectionSummary`.
    ///
    /// The identity `examined == accepted + refused` therefore does **not**
    /// hold for this gate: `examined` is the whole scanned population, and
    /// the projection's two non-version refusal reasons are counted
    /// elsewhere. The identity is exact for the DSM gate.
    pub fn tunnel_build_peer(summary: &CandidateProjectionSummary, accepted: usize) -> Self {
        let refusals = VersionRefusalCounts {
            unknown_version: saturating_u32(summary.refused_unknown_version),
            uncomparable_version: saturating_u32(summary.refused_uncomparable_version),
            below_floor: saturating_u32(summary.refused_below_version_floor),
        };
        Self::from_decision(
            VersionDecisionKind::TunnelBuildPeer,
            saturating_u32(summary.scanned),
            saturating_u32(accepted),
            &refusals,
        )
    }

    /// The receipt for a DSM target decision.
    ///
    /// `examined` is the number of candidates the gate examined and
    /// `accepted` is the number of eligible targets. The DSM gate has exactly
    /// two outcomes — eligible, or refused by a version reason — so
    /// `examined == accepted + refused` holds exactly here.
    pub fn dsm_target(examined: u32, accepted: u32, refusals: &VersionRefusalCounts) -> Self {
        Self::from_decision(VersionDecisionKind::DsmTarget, examined, accepted, refusals)
    }

    /// The receipt for one decision, from the decision's own counts.
    ///
    /// The refusal map and the digest are both derived from the same refusal
    /// counts, so the receipt can never describe counts it does not hold.
    pub(crate) fn from_decision(
        kind: VersionDecisionKind,
        examined: u32,
        accepted: u32,
        refusals: &VersionRefusalCounts,
    ) -> Self {
        let mut by_reason = BTreeMap::new();
        for (label, count) in refusals.counts_by_reason() {
            by_reason.insert(label, count);
        }
        Self {
            kind,
            examined,
            accepted,
            refusals: by_reason,
            digest: version_receipt_digest(kind, examined, accepted, refusals),
        }
    }
}

/// A live-owned, bounded log of peer-eligibility receipts.
///
/// The log holds counts and stable reason labels only — no peer hash, no
/// address, no key, no version string, no payload.
///
/// The owner is shared as a plain `Arc`; the internal mutex is held only for
/// the length of a push or a clone, and never across an `.await`. A poisoned
/// lock degrades to "nothing recorded" and "nothing readable": the owner
/// never panics and never fabricates a decision.
#[derive(Debug)]
pub struct VersionEligibilityOwner {
    receipts: Mutex<VecDeque<VersionEligibilityReceipt>>,
    max_receipts: usize,
}

impl VersionEligibilityOwner {
    /// Constructs an owner that retains at most `max_receipts` receipts.
    ///
    /// A cap of zero records nothing.
    pub fn new(max_receipts: usize) -> Self {
        Self {
            receipts: Mutex::new(VecDeque::new()),
            max_receipts,
        }
    }

    /// Records a receipt, evicting the oldest entry once the log is at its
    /// cap. A cap of zero records nothing.
    pub fn record(&self, receipt: VersionEligibilityReceipt) {
        if self.max_receipts == 0 {
            return;
        }
        if let Ok(mut receipts) = self.receipts.lock() {
            while receipts.len() >= self.max_receipts {
                receipts.pop_front();
            }
            receipts.push_back(receipt);
        }
    }

    /// The receipts, oldest first.
    pub fn receipts(&self) -> Vec<VersionEligibilityReceipt> {
        self.receipts
            .lock()
            .map(|receipts| receipts.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// The number of receipts the owner retains.
    pub fn receipt_count(&self) -> usize {
        self.receipts
            .lock()
            .map(|receipts| receipts.len())
            .unwrap_or(0)
    }

    /// The most recent receipt, if any.
    pub fn last(&self) -> Option<VersionEligibilityReceipt> {
        self.receipts
            .lock()
            .ok()
            .and_then(|receipts| receipts.back().cloned())
    }

    /// Discards every retained receipt.
    pub fn clear(&self) {
        if let Ok(mut receipts) = self.receipts.lock() {
            receipts.clear();
        }
    }
}

/// A digest over one peer-eligibility receipt's counts.
///
/// The digest covers the gate, the population, the accepted count and the
/// refusal counts. It covers no peer identity, because the receipt holds
/// none.
///
/// The byte layout is the gate label, `|`, the examined count, `|`, the
/// accepted count, `|`, then each refusal label, `=`, its count, `,` — in the
/// sorted label order of the refusal map. Plan 503 moved this function
/// without changing a byte of it, so a receipt produced before the move and a
/// receipt produced after it are the same digest.
pub fn version_receipt_digest(
    kind: VersionDecisionKind,
    examined: u32,
    accepted: u32,
    refusals: &VersionRefusalCounts,
) -> [u8; 32] {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(kind.label().as_bytes());
    buffer.push(b'|');
    buffer.extend_from_slice(examined.to_string().as_bytes());
    buffer.push(b'|');
    buffer.extend_from_slice(accepted.to_string().as_bytes());
    buffer.push(b'|');
    for (label, count) in refusals.counts_by_reason() {
        buffer.extend_from_slice(label.as_bytes());
        buffer.push(b'=');
        buffer.extend_from_slice(count.to_string().as_bytes());
        buffer.push(b',');
    }
    let hash = Hash::digest(&buffer);
    let mut digest = [0u8; 32];
    digest.copy_from_slice(hash.as_bytes());
    digest
}

/// Converts a counter to `u32`, saturating at the ceiling.
///
/// A count is never truncated silently and never panics.
fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders bytes as lowercase hex.
    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    }

    /// The golden digest of the Plan 491 private-mesh tunnel-build decision.
    ///
    /// Derived independently from the documented byte layout:
    /// `tunnel-build-peer|5|2|below-floor=1,uncomparable-version=1,unknown-version=1,`
    /// hashed with SHA-256.
    const GOLDEN_TUNNEL_DIGEST: &str =
        "a1c7672f10901a1ff63a0fcad72194e318169b501660fac3c6607dba54b2d73a";

    /// The golden digest of a DSM decision with three below-floor refusals.
    ///
    /// Derived independently from the documented byte layout:
    /// `dsm-target|3|0|below-floor=3,uncomparable-version=0,unknown-version=0,`
    /// hashed with SHA-256.
    const GOLDEN_DSM_DIGEST: &str =
        "6e83031d074a17664a8cace53e258898b22951cb248a94efe8b52b81c7a0c686";

    fn refusal_counts(
        unknown_version: u32,
        uncomparable_version: u32,
        below_floor: u32,
    ) -> VersionRefusalCounts {
        VersionRefusalCounts {
            unknown_version,
            uncomparable_version,
            below_floor,
        }
    }

    #[test]
    fn the_digest_is_unchanged_by_the_move() {
        // The Plan 491 private mesh's tunnel-build decision: five peers
        // examined, two accepted, one refusal per version reason.
        let refusals = refusal_counts(1, 1, 1);
        let digest = version_receipt_digest(VersionDecisionKind::TunnelBuildPeer, 5, 2, &refusals);
        assert_eq!(hex(&digest), GOLDEN_TUNNEL_DIGEST);

        // A DSM decision over the same population, but with every peer
        // refused below the floor.
        let refused = refusal_counts(0, 0, 3);
        let digest = version_receipt_digest(VersionDecisionKind::DsmTarget, 3, 0, &refused);
        assert_eq!(hex(&digest), GOLDEN_DSM_DIGEST);
    }

    #[test]
    fn the_digest_covers_the_decisions_own_counts() {
        let refusals = refusal_counts(1, 1, 1);
        let base = version_receipt_digest(VersionDecisionKind::TunnelBuildPeer, 5, 2, &refusals);
        let other_examined =
            version_receipt_digest(VersionDecisionKind::TunnelBuildPeer, 6, 2, &refusals);
        let other_accepted =
            version_receipt_digest(VersionDecisionKind::TunnelBuildPeer, 5, 3, &refusals);
        let other_gate = version_receipt_digest(VersionDecisionKind::DsmTarget, 5, 2, &refusals);
        let other_refusals = version_receipt_digest(
            VersionDecisionKind::TunnelBuildPeer,
            5,
            2,
            &refusal_counts(2, 1, 1),
        );
        assert_ne!(base, other_examined);
        assert_ne!(base, other_accepted);
        assert_ne!(base, other_gate);
        assert_ne!(base, other_refusals);
    }

    #[test]
    fn the_owner_is_bounded_and_evicts_the_oldest_entry() {
        let owner = VersionEligibilityOwner::new(3);
        for examined in 0..5u32 {
            owner.record(VersionEligibilityReceipt::dsm_target(
                examined,
                0,
                &refusal_counts(0, 0, examined),
            ));
        }
        assert_eq!(owner.receipt_count(), 3);
        let receipts = owner.receipts();
        assert_eq!(receipts.len(), 3);
        // The two oldest entries were evicted; the three newest remain, in
        // order.
        assert_eq!(receipts[0].examined, 2);
        assert_eq!(receipts[1].examined, 3);
        assert_eq!(receipts[2].examined, 4);
        assert_eq!(owner.last().map(|receipt| receipt.examined), Some(4));
    }

    #[test]
    fn a_cap_of_zero_records_nothing() {
        let owner = VersionEligibilityOwner::new(0);
        owner.record(VersionEligibilityReceipt::dsm_target(
            3,
            0,
            &refusal_counts(0, 0, 3),
        ));
        assert_eq!(owner.receipt_count(), 0);
        assert!(owner.receipts().is_empty());
        assert!(owner.last().is_none());
    }

    #[test]
    fn the_owner_can_be_cleared() {
        let owner = VersionEligibilityOwner::new(4);
        owner.record(VersionEligibilityReceipt::dsm_target(
            3,
            0,
            &refusal_counts(0, 0, 3),
        ));
        assert_eq!(owner.receipt_count(), 1);
        owner.clear();
        assert_eq!(owner.receipt_count(), 0);
        assert!(owner.last().is_none());
    }

    #[test]
    fn the_build_constructor_maps_the_three_version_reasons_only() {
        let summary = CandidateProjectionSummary {
            scanned: 9,
            unqualified: 2,
            refused_unknown_version: 1,
            refused_uncomparable_version: 2,
            refused_below_version_floor: 3,
            capped_participation: 1,
        };
        let receipt = VersionEligibilityReceipt::tunnel_build_peer(&summary, 2);
        assert_eq!(receipt.kind, VersionDecisionKind::TunnelBuildPeer);
        assert_eq!(receipt.examined, 9);
        assert_eq!(receipt.accepted, 2);
        assert_eq!(receipt.refused(), 6);
        assert_eq!(receipt.refusals.get("unknown-version"), Some(&1));
        assert_eq!(receipt.refusals.get("uncomparable-version"), Some(&2));
        assert_eq!(receipt.refusals.get("below-floor"), Some(&3));
        // The projection's two non-version refusal reasons are not version
        // refusals: they stay in the summary, and the receipt's map holds the
        // three version reasons only.
        assert_eq!(receipt.refusals.len(), 3);
        assert!(!receipt.refusals.contains_key("unqualified"));
        assert!(!receipt.refusals.contains_key("capped-participation"));
    }

    #[test]
    fn both_producers_use_the_same_stable_reason_labels() {
        let refusals = refusal_counts(1, 2, 3);
        let dsm = VersionEligibilityReceipt::dsm_target(9, 3, &refusals);
        let summary = CandidateProjectionSummary {
            scanned: 9,
            refused_unknown_version: 1,
            refused_uncomparable_version: 2,
            refused_below_version_floor: 3,
            ..CandidateProjectionSummary::default()
        };
        let build = VersionEligibilityReceipt::tunnel_build_peer(&summary, 3);
        let labels: Vec<&str> = refusals.counts_by_reason().keys().copied().collect();
        assert_eq!(
            labels,
            vec!["below-floor", "uncomparable-version", "unknown-version"]
        );
        let dsm_labels: Vec<&str> = dsm.refusals.keys().copied().collect();
        let build_labels: Vec<&str> = build.refusals.keys().copied().collect();
        assert_eq!(dsm_labels, labels);
        assert_eq!(build_labels, labels);
        // The same counts under the same labels produce the same refusal map;
        // only the gate differs.
        assert_eq!(dsm.refusals, build.refusals);
        assert_ne!(dsm.digest, build.digest);
    }

    #[test]
    fn the_dsm_constructor_maps_the_refusal_counts_onto_the_stable_labels() {
        let refusals = refusal_counts(4, 5, 6);
        let receipt = VersionEligibilityReceipt::dsm_target(20, 5, &refusals);
        assert_eq!(receipt.kind, VersionDecisionKind::DsmTarget);
        assert_eq!(receipt.examined, 20);
        assert_eq!(receipt.accepted, 5);
        assert_eq!(receipt.refused(), 15);
        assert_eq!(receipt.refusals.get("unknown-version"), Some(&4));
        assert_eq!(receipt.refusals.get("uncomparable-version"), Some(&5));
        assert_eq!(receipt.refusals.get("below-floor"), Some(&6));
        // The DSM gate has exactly two outcomes, so the identity holds.
        assert_eq!(receipt.examined, receipt.accepted + receipt.refused());
    }

    #[test]
    fn a_receipt_carries_no_peer_identity() {
        let receipt = VersionEligibilityReceipt::dsm_target(9, 3, &refusal_counts(1, 2, 3));
        let rendered = format!("{receipt:?}");
        for forbidden in [
            "RouterHash",
            "RouterIdentity",
            "router_hash",
            "peer_hash",
            "address",
            "session_key",
            "0.9.62",
            "0.9.",
            "127.0.0.1",
            "-----BEGIN",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "a receipt that could name a peer is not a receipt: {rendered}"
            );
        }
        // The receipt's whole content is its gate, its counts, its three
        // stable labels and the digest over them.
        assert_eq!(
            receipt.refusals.keys().copied().collect::<Vec<&str>>(),
            vec!["below-floor", "uncomparable-version", "unknown-version"]
        );
    }

    #[test]
    fn a_count_saturates_instead_of_truncating() {
        // A `usize` count that does not fit in a `u32` saturates; it never
        // wraps and never panics.
        let summary = CandidateProjectionSummary {
            scanned: usize::MAX,
            refused_unknown_version: usize::MAX,
            ..CandidateProjectionSummary::default()
        };
        let receipt = VersionEligibilityReceipt::tunnel_build_peer(&summary, usize::MAX);
        assert_eq!(receipt.examined, u32::MAX);
        assert_eq!(receipt.accepted, u32::MAX);
        assert_eq!(receipt.refusals.get("unknown-version"), Some(&u32::MAX));
    }
}
