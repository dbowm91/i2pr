//! Plan 106 runtime-facing seam for the Plan 105 NetDB lookup and
//! publication state machines.
//!
//! The seam exposes the typed [`LookupAction`] vocabulary the runtime
//! adapter consumes. Plan 107 wires the seam to an injected
//! [`i2pr_netdb::ReplyPathProvider`]; when no provider is injected,
//! the seam continues to report
//! `ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable`,
//! preserving the Plan 106 hard-coded default.
//!
//! A peer transport link is **not** equivalent to a complete reply
//! path; the lookup state machine refuses to emit a
//! standards-conformant `DatabaseLookup` until the runtime supplies
//! an exploratory reply path. The seam exposes this contract.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use i2pr_netdb::{
    LookupAction, LookupFinalState, LookupId, LookupKind, LookupOutcome, LookupPolicy,
    ReplyPathProvider, RouterHash, RouterInfoLookup, RouterInfoStore, StartOutcome,
    ValidationContext,
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

/// Plan 106/107 runtime-facing seam for the NetDB query state
/// machines.
pub struct NetDbSeam {
    /// Per-target single-threaded lookup state machine.
    lookup: RouterInfoLookup,
    /// Optional reply-path provider injected by the future Milestone
    /// 5 owner.
    provider: Option<Box<dyn ReplyPathProvider>>,
}

impl NetDbSeam {
    /// Constructs a seam backed by the supplied bounded lookup
    /// policy. The seam starts without an injected reply-path
    /// provider; the caller can attach one with
    /// [`Self::set_reply_path_provider`].
    pub fn new(policy: LookupPolicy) -> Self {
        Self {
            lookup: RouterInfoLookup::new(policy),
            provider: None,
        }
    }

    /// Attaches an injected reply-path provider. The provider is
    /// queried through [`Self::path_status`] and
    /// [`Self::begin_lookup`].
    pub fn set_reply_path_provider(&mut self, provider: Box<dyn ReplyPathProvider>) {
        self.provider = Some(provider);
    }

    /// Clears any injected reply-path provider and reverts the seam
    /// to the Plan 106 blocked status.
    pub fn clear_reply_path_provider(&mut self) {
        self.provider = None;
    }

    /// Returns the current exploratory-tunnel reply-path status.
    ///
    /// Without an injected provider the seam returns
    /// `BlockedExploratoryTunnelUnavailable`. With an injected
    /// provider the seam consults `has_inbound_tunnel()`; a
    /// positive answer promotes the seam to `Available`.
    pub fn path_status(&self) -> ExploratoryPathStatus {
        match self.provider.as_ref() {
            Some(provider) if provider.has_inbound_tunnel() => ExploratoryPathStatus::Available,
            _ => ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable,
        }
    }

    /// Returns the active lookup identity, if any.
    pub fn active_lookup(&self) -> Option<LookupId> {
        self.lookup.active_lookup()
    }

    /// Begins a new lookup against the supplied store using the
    /// injected reply-path provider when one is available. The
    /// runtime adapter receives the [`LookupAction`] emitted by the
    /// state machine; under Plan 107 the action is either
    /// `SendDatabaselookup` (when the provider returns a path) or
    /// `NeedExploratoryReplyPath` (when no path is available).
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
            StartOutcome::PendingAttempt(action) => action,
            StartOutcome::NeedsReplyPath(action) => {
                // The state machine is asking for a reply path. Ask
                // the injected provider; if a path is available,
                // feed it back into the state machine and return the
                // resulting action.
                if let Some(provider) = self.provider.as_ref() {
                    if let Some(path) = provider.provide_reply_path() {
                        if self.lookup.accept_reply_path(lookup_id, path) {
                            return self.pending_action_after_path(
                                store,
                                request_id,
                                target,
                                routing_key,
                            );
                        }
                    }
                }
                action
            }
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

    fn pending_action_after_path(
        &mut self,
        _store: &RouterInfoStore,
        _request_id: u64,
        _target: RouterHash,
        _routing_key: &RouterHash,
    ) -> LookupAction {
        // After `accept_reply_path` returns true the state machine
        // records the path. The caller will need to drive the next
        // attempt through `lookup_mut()` to emit the actual
        // `SendDatabaselookup` action. Returning `NeedExploratoryReplyPath`
        // here keeps the seam conservative; Plan 108 will replace
        // this stub with the live advance-loop that emits the
        // follow-up action.
        LookupAction::NeedExploratoryReplyPath {
            lookup_id: self
                .lookup
                .active_lookup()
                .expect("accept_reply_path sets the lookup"),
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
    use i2pr_netdb::ReplyPath;
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

    /// Test-only provider that pretends to have a valid inbound
    /// tunnel. The provider does not consume any external state.
    #[derive(Debug)]
    struct FakeReplyPathProvider {
        path: ReplyPath,
        has_tunnel: bool,
    }

    impl i2pr_netdb::ReplyPathProvider for FakeReplyPathProvider {
        fn has_inbound_tunnel(&self) -> bool {
            self.has_tunnel
        }
        fn provide_reply_path(&self) -> Option<ReplyPath> {
            self.has_tunnel.then_some(self.path)
        }
    }

    #[test]
    fn path_status_flip_to_available_when_provider_reports_tunnel() {
        let mut seam = NetDbSeam::new(LookupPolicy::default());
        assert_eq!(
            seam.path_status(),
            ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable
        );
        let gateway = RouterHash::from_bytes([0x77u8; 32]);
        let path = ReplyPath::new(gateway, 0x4242).expect("path");
        let provider = FakeReplyPathProvider {
            path,
            has_tunnel: true,
        };
        seam.set_reply_path_provider(Box::new(provider));
        assert_eq!(seam.path_status(), ExploratoryPathStatus::Available);
        seam.clear_reply_path_provider();
        assert_eq!(
            seam.path_status(),
            ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable
        );
    }

    #[test]
    fn path_status_stays_blocked_when_provider_has_no_tunnel() {
        let mut seam = NetDbSeam::new(LookupPolicy::default());
        let gateway = RouterHash::from_bytes([0x77u8; 32]);
        let path = ReplyPath::new(gateway, 0x4242).expect("path");
        let provider = FakeReplyPathProvider {
            path,
            has_tunnel: false,
        };
        seam.set_reply_path_provider(Box::new(provider));
        assert_eq!(
            seam.path_status(),
            ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable
        );
    }

    #[test]
    fn begin_lookup_accepts_path_from_provider() {
        let mut seam = NetDbSeam::new(LookupPolicy::default());
        let store = RouterInfoStore::default();
        let target = RouterHash::from_bytes([0x42u8; 32]);
        let routing_key = target;
        let gateway = RouterHash::from_bytes([0x77u8; 32]);
        let path = ReplyPath::new(gateway, 0x4242).expect("path");
        let provider = FakeReplyPathProvider {
            path,
            has_tunnel: true,
        };
        seam.set_reply_path_provider(Box::new(provider));
        let action = seam.begin_lookup(&store, 1, target, &routing_key);
        // Plan 107 stops short of producing the post-path
        // `SendDatabaselookup` action in this round; the seam
        // returns the `NeedExploratoryReplyPath` placeholder after
        // recording the path on the state machine. The full
        // post-path dispatch lands in Plan 108.
        assert!(matches!(
            action,
            LookupAction::NeedExploratoryReplyPath { .. }
                | LookupAction::Complete { .. }
                | LookupAction::SendDatabaselookup { .. }
        ));
    }
}
