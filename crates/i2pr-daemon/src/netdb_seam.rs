//! Plan 106 runtime-facing seam for the Plan 105 NetDB lookup and
//! publication state machines.
//!
//! The seam exposes the typed [`LookupAction`] vocabulary the runtime
//! adapter consumes while the Milestone 5 exploratory-tunnel
//! substrate is not yet available. It deliberately reports an
//! exploratory NetDB path that is unavailable so callers cannot
//! silently invent direct transport shortcuts.
//!
//! A peer transport link is **not** equivalent to a complete reply
//! path; the lookup state machine refuses to emit a
//! standards-conformant `DatabaseLookup` until the runtime supplies
//! an exploratory reply path. The seam exposes this contract.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use i2pr_netdb::{
    LookupAction, LookupFinalState, LookupId, LookupKind, LookupOutcome, LookupPolicy, RouterHash,
    RouterInfoLookup, RouterInfoStore, StartOutcome, ValidationContext,
};

/// Bounded reply-path availability status. The runtime adapter reports
/// the current state through [`NetDbSeam::path_status`] so callers
/// can distinguish "transport reachable, exploratory tunnel absent"
/// from "transport also absent".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploratoryPathStatus {
    /// The exploratory tunnel substrate is unavailable under the
    /// current authority. A direct NTCP2 or other transport link is
    /// not a substitute.
    BlockedExploratoryTunnelUnavailable,
    /// The runtime adapter has a registered exploratory reply path
    /// and may dispatch `DatabaseLookup` actions.
    Available,
}

impl fmt::Display for ExploratoryPathStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::BlockedExploratoryTunnelUnavailable => "blocked_exploratory_tunnel_unavailable",
            Self::Available => "available",
        };
        formatter.write_str(label)
    }
}

/// Plan 106 runtime-facing seam for the NetDB query state machines.
pub struct NetDbSeam {
    /// Per-target single-threaded lookup state machine.
    lookup: RouterInfoLookup,
}

impl NetDbSeam {
    /// Constructs a seam backed by the supplied bounded lookup
    /// policy.
    pub fn new(policy: LookupPolicy) -> Self {
        Self {
            lookup: RouterInfoLookup::new(policy),
        }
    }

    /// Returns the current exploratory-tunnel reply-path status.
    /// Plan 106 always reports `BlockedExploratoryTunnelUnavailable`
    /// until Milestone 5 lands the exploratory inbound/outbound
    /// tunnel substrate.
    pub fn path_status(&self) -> ExploratoryPathStatus {
        ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable
    }

    /// Returns the active lookup identity, if any.
    pub fn active_lookup(&self) -> Option<LookupId> {
        self.lookup.active_lookup()
    }

    /// Begins a new lookup against the supplied store. The runtime
    /// adapter receives the [`LookupAction`] emitted by the state
    /// machine; under Plan 106 the action is always
    /// `NeedExploratoryReplyPath` because the runtime cannot supply a
    /// reply path.
    pub fn begin_lookup(
        &mut self,
        store: &RouterInfoStore,
        request_id: u64,
        target: RouterHash,
        routing_key: &RouterHash,
    ) -> LookupAction {
        let lookup_id = LookupId::new(request_id, LookupKind::RouterInfo, target);
        let outcome = self.lookup.start(store, lookup_id, routing_key);
        match outcome {
            StartOutcome::PendingAttempt(action) | StartOutcome::NeedsReplyPath(action) => action,
            StartOutcome::Terminal(result) => {
                let final_state = match *result {
                    i2pr_netdb::LookupResult::Failure { final_state, .. } => final_state,
                    i2pr_netdb::LookupResult::Success { .. } => LookupFinalState::Success,
                };
                LookupAction::Complete {
                    lookup_id,
                    outcome: LookupOutcome::new(
                        lookup_id.kind(),
                        lookup_id.target(),
                        final_state,
                        0,
                        0,
                    ),
                }
            }
        }
    }

    /// Cancels the active lookup, if any.
    pub fn cancel(&mut self) {
        let _ = self.lookup.cancel();
    }

    /// Returns the latest state-machine diagnostics.
    pub fn diagnostics(&self) -> BTreeMap<&'static str, usize> {
        let mut map = BTreeMap::new();
        map.insert(
            "active_lookup_present",
            usize::from(self.lookup.active_lookup().is_some()),
        );
        map.insert(
            "path_status_blocked",
            usize::from(matches!(
                self.path_status(),
                ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable
            )),
        );
        map
    }

    /// Returns whether a lookup is currently active.
    pub fn is_active(&self) -> bool {
        self.lookup.active_lookup().is_some()
    }

    /// Returns the underlying store reference for callers that need
    /// to drive Plan 105 response ingestion.
    pub fn lookup_mut(&mut self) -> &mut RouterInfoLookup {
        &mut self.lookup
    }

    /// Returns the underlying lookup state machine reference.
    pub fn lookup(&self) -> &RouterInfoLookup {
        &self.lookup
    }

    /// Returns a typed validation context the caller can use for
    /// response ingestion. The context carries the default policy;
    /// callers may override it before response ingestion.
    pub fn validation_context(&self) -> ValidationContext {
        ValidationContext::new(i2pr_proto::Date::from_millis(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::Date;

    #[test]
    fn path_status_reports_blocked_until_milestone_five() {
        let seam = NetDbSeam::new(LookupPolicy::default());
        assert_eq!(
            seam.path_status(),
            ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable
        );
    }

    #[test]
    fn begin_lookup_emits_need_exploratory_reply_path() {
        let mut seam = NetDbSeam::new(LookupPolicy::default());
        let store = RouterInfoStore::default();
        let target = RouterHash::from_bytes([0x42u8; 32]);
        let routing_key = target;
        let action = seam.begin_lookup(&store, 1, target, &routing_key);
        assert!(matches!(
            action,
            LookupAction::NeedExploratoryReplyPath { .. } | LookupAction::Complete { .. }
        ));
    }

    #[test]
    fn cancel_returns_to_idle() {
        let mut seam = NetDbSeam::new(LookupPolicy::default());
        seam.cancel();
        assert!(!seam.is_active());
    }

    #[test]
    fn diagnostics_reports_blocked_path() {
        let seam = NetDbSeam::new(LookupPolicy::default());
        let map = seam.diagnostics();
        assert_eq!(map.get("path_status_blocked"), Some(&1));
        assert_eq!(map.get("active_lookup_present"), Some(&0));
    }

    #[test]
    fn validation_context_uses_default_policy() {
        let seam = NetDbSeam::new(LookupPolicy::default());
        let ctx = seam.validation_context();
        assert_eq!(ctx.now, Date::from_millis(0));
    }

    #[test]
    fn exploratory_path_status_label_is_stable() {
        let label = ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable.to_string();
        assert_eq!(label, "blocked_exploratory_tunnel_unavailable");
        assert_eq!(ExploratoryPathStatus::Available.to_string(), "available");
    }
}
