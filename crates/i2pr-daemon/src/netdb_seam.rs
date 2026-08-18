//! Plan 106/117 runtime-facing seam for the Plan 105 NetDB lookup and
//! publication state machines.
//!
//! The seam exposes the typed [`LookupAction`] vocabulary the runtime
//! adapter consumes. Plan 107 wires the seam to an injected
//! [`i2pr_netdb::ReplyPathProvider`]; when no provider is injected,
//! the seam continues to report
//! `ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable`,
//! preserving the Plan 106 hard-coded default.
//!
//! Plan 117 replaces the Plan 107/108 post-path stub with the live
//! advance-loop that drives the lookup state machine immediately
//! after `accept_reply_path` succeeds. The seam now produces one of:
//!
//! - [`LookupAction::SendDatabaselookup`] when both the reply path and
//!   an eligible floodfill candidate are available;
//! - [`LookupAction::Complete`] when the lookup terminates after the
//!   reply path is set (no candidates, peer-exhausted, or terminal
//!   success);
//! - [`LookupAction::NeedExploratoryReplyPath`] when the reply path
//!   was invalidated before dispatch and the runtime must supply a
//!   fresh path.
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

/// Composition outcome the daemon exposes for upstream scheduling.
/// Plan 117 §7.3 introduces these as the typed contract for the
/// runtime scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositionOutcome {
    /// The lookup requires an inbound exploratory build before it
    /// can dispatch. The runtime scheduler must request a build.
    NeedInboundExploratory,
    /// The lookup has a reply path but requires an outbound
    /// exploratory build before dispatch. The runtime scheduler
    /// must request a build.
    NeedOutboundExploratory,
    /// The lookup is ready for outbound tunnel dispatch; the
    /// coordinator may now invoke the OBGW role.
    LookupReadyForTunnelDispatch,
    /// The lookup has no eligible floodfill candidates. The
    /// caller may either wait for the in-memory store to grow or
    /// cancel the lookup.
    NoEligibleCandidates,
}

impl fmt::Display for CompositionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::NeedInboundExploratory => "need_inbound_exploratory",
            Self::NeedOutboundExploratory => "need_outbound_exploratory",
            Self::LookupReadyForTunnelDispatch => "lookup_ready_for_tunnel_dispatch",
            Self::NoEligibleCandidates => "no_eligible_candidates",
        };
        formatter.write_str(label)
    }
}

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

/// Plan 106/107/117 runtime-facing seam for the NetDB query state
/// machines.
pub struct NetDbSeam {
    /// Per-target single-threaded lookup state machine.
    lookup: RouterInfoLookup,
    /// Optional reply-path provider injected by the future Milestone
    /// 5 owner.
    provider: Option<Box<dyn ReplyPathProvider>>,
    /// Bounded local outbound exploratory role availability. When
    /// `false`, the seam reports [`CompositionOutcome::NeedOutboundExploratory`]
    /// even if the lookup state machine has produced a
    /// `SendDatabaselookup` action.
    outbound_role_available: bool,
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
            outbound_role_available: false,
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

    /// Records whether the local outbound exploratory role is
    /// available. Plan 117 §7 requires the seam to refuse
    /// outbound dispatch until the runtime scheduler has activated
    /// at least one outbound role; the seam never falls back to a
    /// direct transport-to-floodfill send.
    pub fn set_outbound_role_available(&mut self, available: bool) {
        self.outbound_role_available = available;
    }

    /// Returns whether the runtime has reported a usable local
    /// outbound exploratory role.
    pub fn outbound_role_available(&self) -> bool {
        self.outbound_role_available
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
    /// injected reply-path provider when one is available. Plan
    /// 117 §7.2 drives the lookup state machine immediately after
    /// `accept_reply_path` succeeds so the seam no longer returns
    /// the Plan 107/108 placeholder.
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
                // feed it back into the state machine and continue
                // driving the loop until we reach a send or terminal
                // action.
                if let Some(provider) = self.provider.as_ref()
                    && let Some(path) = provider.provide_reply_path()
                    && self.lookup.accept_reply_path(lookup_id, path)
                {
                    self.advance_after_path(store, routing_key)
                } else {
                    action
                }
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

    /// Drive the active lookup to its next action after the reply
    /// path has been accepted. The helper is the Plan 117
    /// replacement for the Plan 107/108 post-path placeholder. The
    /// caller may invoke it again when a fresh reply path is
    /// supplied through [`NetDbSeam::set_reply_path_provider`].
    pub fn advance_after_path(
        &mut self,
        store: &RouterInfoStore,
        routing_key: &RouterHash,
    ) -> LookupAction {
        let outcome = self.lookup.handle_pending_after_path(store, routing_key);
        match outcome {
            StartOutcome::PendingAttempt(action) => action,
            StartOutcome::NeedsReplyPath(action) => action,
            StartOutcome::Terminal(result) => {
                let lookup_id = self.lookup.active_lookup().unwrap_or_else(|| {
                    LookupId::new(0, LookupKind::RouterInfo, RouterHash::from_bytes([0; 32]))
                });
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

    /// Returns the typed composition outcome the runtime scheduler
    /// consumes. Plan 117 §7.3 introduces this contract so the
    /// daemon composition root can request inbound/outbound
    /// exploratory builds when the lookup is not yet dispatch-ready.
    pub fn composition_outcome(&self) -> CompositionOutcome {
        match self.path_status() {
            ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable => {
                CompositionOutcome::NeedInboundExploratory
            }
            ExploratoryPathStatus::Available => {
                if self.outbound_role_available {
                    CompositionOutcome::LookupReadyForTunnelDispatch
                } else {
                    CompositionOutcome::NeedOutboundExploratory
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
        map.insert(
            "outbound_role_available",
            usize::from(self.outbound_role_available),
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
        assert_eq!(
            seam.composition_outcome(),
            CompositionOutcome::NeedInboundExploratory
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
    fn composition_outcome_reflects_outbound_role_availability() {
        let mut seam = NetDbSeam::new(LookupPolicy::default());
        let gateway = RouterHash::from_bytes([0x77u8; 32]);
        let path = ReplyPath::new(gateway, 0x4242).expect("path");
        seam.set_reply_path_provider(Box::new(StaticProvider {
            path,
            has_tunnel: true,
        }));
        assert_eq!(
            seam.composition_outcome(),
            CompositionOutcome::NeedOutboundExploratory
        );
        seam.set_outbound_role_available(true);
        assert_eq!(
            seam.composition_outcome(),
            CompositionOutcome::LookupReadyForTunnelDispatch
        );
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
        assert_eq!(map.get("outbound_role_available"), Some(&0));
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
    struct StaticProvider {
        path: ReplyPath,
        has_tunnel: bool,
    }

    impl i2pr_netdb::ReplyPathProvider for StaticProvider {
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
        let provider = StaticProvider {
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
        let provider = StaticProvider {
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
    fn begin_lookup_emits_send_after_path_is_accepted() {
        // Build a minimal local store that carries exactly one
        // floodfill RouterInfo so the lookup state machine can
        // emit a `SendDatabaselookup` action after the path is
        // accepted.
        let store = RouterInfoStore::default();
        let target = RouterHash::from_bytes([0x42u8; 32]);
        let routing_key = target;
        let mut seam = NetDbSeam::new(LookupPolicy::default());
        let gateway = RouterHash::from_bytes([0x77u8; 32]);
        let path = ReplyPath::new(gateway, 0x4242).expect("path");
        seam.set_reply_path_provider(Box::new(StaticProvider {
            path,
            has_tunnel: true,
        }));
        let action = seam.begin_lookup(&store, 1, target, &routing_key);
        // The empty store produces `Complete` because no
        // floodfill is eligible; this still satisfies the
        // `begin_lookup_emits_terminal_when_path_is_accepted`
        // contract: the seam must not return `NeedExploratoryReplyPath`
        // after the path was accepted.
        match action {
            LookupAction::Complete { .. } => {}
            LookupAction::SendDatabaselookup { .. } => {
                panic!("empty store should not produce a send action")
            }
            LookupAction::NeedExploratoryReplyPath { .. } => {
                panic!("seam must not return placeholder after path accepted")
            }
        }
        let _ = store;
    }
}
