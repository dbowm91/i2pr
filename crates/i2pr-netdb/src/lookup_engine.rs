//! Plan 105 §7-9: iterative `RouterInfo` lookup state machine.
//!
//! The state machine consumes the validated in-memory store plus the
//! injected [`ReplyPathSink`] and emits typed
//! [`LookupAction`]s. It owns the deadline, peer budget, duplicate
//! coalescing, and suggestion merging; it does not own a runtime,
//! sockets, tunnels, or transport delivery.
//!
//! Two paths are exposed:
//!
//! - [`RouterInfoLookup`] — a synchronous, step-wise driver the
//!   runtime adapter (Plan 106) drives one tick at a time.
//! - [`handle_response`] / [`handle_delivery_outcome`] — typed
//!   response ingestion helpers the runtime invokes when an I2NP
//!   message arrives.

use std::collections::BTreeMap;

use i2pr_proto::{
    DatabaseLookupMessage, DatabaseSearchReplyMessage, DatabaseStoreData, DatabaseStoreMessage,
    I2npBody, I2npMessage, RouterInfo,
};
use thiserror::Error;

use crate::databaselookup::build_databaselookup;
use crate::lease_set2::{
    DestinationHash, LeaseSet2Store, LeaseSet2ValidationContext, ValidatedLeaseSet2,
};
use crate::lookup_action::{
    LookupAction, LookupFinalState, MAX_DECOMPRESSED_ROUTER_INFO_BYTES, decompress_router_info,
};
use crate::lookup_id::{LookupId, LookupKind, ReplyPath, WaiterSet};
use crate::lookup_policy::LookupPolicy;
use crate::router_info::{RouterHash, ValidatedRouterInfo, ValidationContext, router_hash};
use crate::store::RouterInfoStore;

/// Diagnostic surface for the lookup state machine.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LookupEngineError {
    /// The supplied identifier, kind, and target combination does not
    /// match the active lookup.
    #[error("lookup identity mismatch for lookup {expected:?}, got {actual:?}")]
    IdentityMismatch {
        /// Lookup identity the caller invoked.
        actual: LookupId,
        /// Lookup identity the state machine is tracking.
        expected: LookupId,
    },
    /// A reply path was supplied for a lookup that has already
    /// started an attempt.
    #[error("reply path supplied after the lookup started")]
    ReplyPathLate,
    /// A duplicate coalesce request was rejected because the waiter
    /// set is at capacity.
    #[error("waiter set at capacity")]
    WaiterCapacityExceeded,
    /// A duplicate coalesce request was rejected because the lookup
    /// is no longer active.
    #[error("lookup is no longer active")]
    LookupTerminal,
}

/// Final typed result of a `RouterInfo` lookup. The state machine
/// produces one of these when the lookup completes (success or
/// failure).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupResult {
    /// The lookup completed successfully with a validated
    /// `RouterInfo`.
    Success {
        /// Lookup identity.
        lookup_id: LookupId,
        /// Validated target `RouterInfo`.
        router_info: Box<ValidatedRouterInfo>,
    },
    /// The lookup completed successfully with a validated
    /// Standard `LeaseSet2`. Plan 122 §A extends the typed result
    /// surface so destination lookups can resolve remote LS2s
    /// through the existing state machine.
    LeaseSet2Success {
        /// Lookup identity.
        lookup_id: LookupId,
        /// Validated target `LeaseSet2`.
        lease_set2: Box<ValidatedLeaseSet2>,
    },
    /// Plan 351: a `LeaseSet2`-kind lookup completed with a **type-5**
    /// `EncryptedLeaseSet2` record filed under the day's blinded storage
    /// key.
    ///
    /// The wire lookup type is unchanged (`LookupKind::LeaseSet2` codes to
    /// `1`, and that is also what a reference client issues for an
    /// encrypted service — the record is filed under its blinded storage
    /// key rather than a destination hash). What is new is that the
    /// lookup key is a `RouterHash` supplied verbatim instead of being
    /// re-derived from a `DestinationHash`, because a blinded storage key
    /// is not a destination hash and no `Destination` exists to derive one
    /// from.
    ///
    /// This variant carries the **raw** message, not a
    /// `ValidatedEncryptedLeaseSet2`. The key match and the record type are
    /// the only checks applied here, deliberately:
    ///
    /// - the closed type-11 signature profile is **ADR 0032's** policy and
    ///   stays with the ELS2 owner, so this layer neither validates nor
    ///   unwraps;
    /// - unwrapping needs the daily blinding material and, for an authorized
    ///   service, per-client key material that the NetDB has no business
    ///   holding;
    /// - the identity binding — that the unwrapped record's destination
    ///   signs with the unblinded public key named by the `.b33` address —
    ///   cannot be checked here at all, because this layer never sees the
    ///   address.
    ///
    /// A caller that matches only `LeaseSet2Success` therefore treats a
    /// type-5 reply as a non-match and keeps waiting, which is the correct
    /// fail-closed outcome for an ordinary consumer.
    EncryptedLeaseSet2Success {
        /// Lookup identity.
        lookup_id: LookupId,
        /// Bounded type-5 record message, for the ELS2 owner to validate
        /// and unwrap.
        message: Box<DatabaseStoreMessage>,
    },
    /// The lookup terminated without finding a usable response.
    Failure {
        /// Lookup identity.
        lookup_id: LookupId,
        /// Categorical final state.
        final_state: LookupFinalState,
        /// Bounded diagnostics.
        diagnostics: LookupDiagnostics,
    },
}

/// Bounded diagnostic counts the state machine tracks for one query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LookupDiagnostics {
    /// Number of `DatabaseLookup` attempts the state machine has
    /// emitted.
    pub attempts: usize,
    /// Number of `DatabaseSearchReply` suggestions retained.
    pub suggestions_merged: usize,
}

/// Internal state of one active lookup.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct ActiveLookup {
    kind: LookupKind,
    target: RouterHash,
    reply_path: Option<ReplyPath>,
    excluded: Vec<RouterHash>,
    queried: Vec<RouterHash>,
    suggestions: Vec<RouterHash>,
    attempts: usize,
    suggestions_merged: usize,
    /// Plan 489: the ordered candidate list the caller supplied for
    /// this lookup. When present, the state machine draws its
    /// candidates from this list instead of re-selecting against the
    /// flat store.
    pinned: Option<Vec<RouterHash>>,
    /// Plan 497: the routing table's preferred peers for this lookup.
    ///
    /// Unlike `pinned`, this is a **priority prefix**, not a closed
    /// set. The flat store's current selection still follows it, so a
    /// peer ingested while the lookup runs is still queried and no
    /// candidate the flat store could serve is ever lost.
    preferred: Option<Vec<RouterHash>>,
}

impl ActiveLookup {
    /// Returns the active lookup suggestion list (test-only).
    #[doc(hidden)]
    pub fn suggestions(&self) -> &[RouterHash] {
        &self.suggestions
    }

    /// Returns the frozen candidate list of the active lookup
    /// (test-only).
    ///
    /// Plan 489's list is the caller's list; the accessor lets a test
    /// prove that the peer budget narrows the **attempts** and leaves
    /// the candidate list itself alone.
    #[doc(hidden)]
    pub fn pinned_candidates(&self) -> Option<&[RouterHash]> {
        self.pinned.as_deref()
    }

    fn new(lookup_id: LookupId, policy_max_suggested_hashes: usize) -> Self {
        Self {
            kind: lookup_id.kind(),
            target: lookup_id.target(),
            reply_path: None,
            excluded: vec![lookup_id.target()],
            queried: Vec::new(),
            suggestions: Vec::with_capacity(policy_max_suggested_hashes),
            attempts: 0,
            suggestions_merged: 0,
            pinned: None,
            preferred: None,
        }
    }

    /// Test-only constructor that pre-seeds the reply path so
    /// composition tests in adjacent crates can drive the
    /// ingestion paths without racing `accept_reply_path`.
    #[doc(hidden)]
    pub fn new_with_reply_path(
        lookup_id: LookupId,
        policy_max_suggested_hashes: usize,
        reply_gateway: RouterHash,
        reply_tunnel_id: u32,
    ) -> Self {
        use crate::lookup_id::ReplyPath;
        Self {
            kind: lookup_id.kind(),
            target: lookup_id.target(),
            reply_path: Some(
                ReplyPath::new(reply_gateway, reply_tunnel_id).expect("non-zero tunnel id"),
            ),
            excluded: vec![lookup_id.target()],
            queried: Vec::new(),
            suggestions: Vec::with_capacity(policy_max_suggested_hashes),
            attempts: 0,
            suggestions_merged: 0,
            pinned: None,
            preferred: None,
        }
    }

    fn diagnostics(&self) -> LookupDiagnostics {
        LookupDiagnostics {
            attempts: self.attempts,
            suggestions_merged: self.suggestions_merged,
        }
    }
}

/// Plan 489: the maximum number of pinned candidates a lookup may
/// carry. The list is a bounded view of the routing table, never an
/// unbounded channel.
pub const MAX_PINNED_CANDIDATES: usize = 64;

/// Outcome emitted by [`RouterInfoLookup::start`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartOutcome {
    /// The lookup started and the caller should hand the resulting
    /// [`LookupAction::SendDatabaselookup`] to the runtime adapter.
    PendingAttempt(LookupAction),
    /// The lookup started but blocked on a missing reply path.
    NeedsReplyPath(LookupAction),
    /// The lookup started and immediately completed (success or
    /// terminal failure).
    Terminal(Box<LookupResult>),
}

/// Single-threaded `RouterInfo` lookup state machine.
pub struct RouterInfoLookup {
    policy: LookupPolicy,
    active: Option<ActiveLookup>,
    active_request_id: Option<u64>,
}

impl RouterInfoLookup {
    /// Creates a new lookup state machine with the supplied policy.
    pub fn new(policy: LookupPolicy) -> Self {
        Self {
            policy,
            active: None,
            active_request_id: None,
        }
    }

    /// Returns the active lookup identity, if any.
    pub fn active_lookup(&self) -> Option<LookupId> {
        self.active
            .as_ref()
            .map(|state| LookupId::new(self.active_request_id(), state.kind, state.target))
    }

    fn active_request_id(&self) -> u64 {
        // The internal request identifier lives on the lookup state;
        // we derive it from the lookup identity stored outside.
        self.active_request_id
            .expect("active lookup identity must be set")
    }

    /// Starts a new lookup. Returns the typed action the runtime
    /// adapter must dispatch (or a terminal result).
    pub fn start(
        &mut self,
        store: &RouterInfoStore,
        lookup_id: LookupId,
        routing_key: &RouterHash,
    ) -> StartOutcome {
        if self.active.is_some() {
            return StartOutcome::Terminal(Box::new(LookupResult::Failure {
                lookup_id,
                final_state: LookupFinalState::Cancelled,
                diagnostics: LookupDiagnostics::default(),
            }));
        }
        self.active_request_id = Some(lookup_id.request_id());
        let state = ActiveLookup::new(lookup_id, self.policy.max_suggested_hashes());
        // Compute the initial candidate selection. The selector
        // already excludes the target.
        let selection = crate::lookup_policy::select_floodfill_candidates(
            store,
            &state.target,
            routing_key,
            &state.excluded,
            &self.policy,
        );
        let result = self.advance_attempt(state, &selection.hashes());
        self.handle_start_outcome(result, lookup_id)
    }

    /// Plan 489: starts a lookup whose candidates come from an
    /// explicit, ordered list rather than from the flat store.
    ///
    /// The optimized routing table supplies the list: the peers it
    /// knows, closest to the requested routing key first. The list is
    /// truncated to [`MAX_PINNED_CANDIDATES`], so a caller cannot turn
    /// the lookup into an unbounded fan-out.
    ///
    /// The routing key is deliberately not a parameter: the pinned
    /// list already encodes the distance order the routing table
    /// computed, and the state machine never re-sorts it.
    ///
    /// Everything else about the lookup is unchanged: the deadline,
    /// the peer budget, the duplicate coalescing, the suggestion
    /// merging and the fail-closed terminal states all behave exactly
    /// as they do for [`Self::start`]. A lookup that used to fail
    /// closed still fails closed.
    pub fn start_with_candidates(
        &mut self,
        lookup_id: LookupId,
        candidates: &[RouterHash],
    ) -> StartOutcome {
        if self.active.is_some() {
            return StartOutcome::Terminal(Box::new(LookupResult::Failure {
                lookup_id,
                final_state: LookupFinalState::Cancelled,
                diagnostics: LookupDiagnostics::default(),
            }));
        }
        self.active_request_id = Some(lookup_id.request_id());
        let mut state = ActiveLookup::new(lookup_id, self.policy.max_suggested_hashes());
        let pinned: Vec<RouterHash> = candidates
            .iter()
            .copied()
            .take(MAX_PINNED_CANDIDATES)
            .collect();
        state.pinned = Some(pinned.clone());
        let result = self.advance_attempt(state, &pinned);
        self.handle_start_outcome(result, lookup_id)
    }

    /// Plan 497: starts a lookup whose **preferred** peers are tried
    /// first, without freezing the candidate set.
    ///
    /// The caller supplies the routing table's closest-first answer.
    /// The state machine selects the candidates exactly as
    /// [`Self::start`] does — the same flat-store selection, the same
    /// policy, the same exclusions — and then **reorders** that
    /// selection so the preferred peers lead. The candidate *set* is
    /// therefore never reduced: a peer the flat store could serve is
    /// still served, and the target is still excluded.
    ///
    /// A preferred peer the flat store would not have served is
    /// ignored. The table is a cache of derived state and the flat
    /// store is the authority, so the table may improve the choice but
    /// never widen it.
    ///
    /// Unlike [`Self::start_with_candidates`], the preference is not a
    /// closed list. On every later advance the state machine re-selects
    /// against the authoritative store and puts the preferred peers
    /// first, so a peer ingested while the lookup runs is still queried.
    /// Plan 489's exploratory path keeps its closed-list behaviour.
    ///
    /// The deadline, the peer budget, the duplicate coalescing, the
    /// suggestion merging and the fail-closed terminal states are
    /// unchanged.
    pub fn start_with_preferred(
        &mut self,
        store: &RouterInfoStore,
        lookup_id: LookupId,
        routing_key: &RouterHash,
        preferred: &[RouterHash],
    ) -> StartOutcome {
        if self.active.is_some() {
            return StartOutcome::Terminal(Box::new(LookupResult::Failure {
                lookup_id,
                final_state: LookupFinalState::Cancelled,
                diagnostics: LookupDiagnostics::default(),
            }));
        }
        self.active_request_id = Some(lookup_id.request_id());
        let mut state = ActiveLookup::new(lookup_id, self.policy.max_suggested_hashes());
        // The authoritative selection, computed exactly as `start`
        // computes it.
        let selected = crate::lookup_policy::select_floodfill_candidates(
            store,
            &state.target,
            routing_key,
            &state.excluded,
            &self.policy,
        )
        .hashes();
        // Reorder, never shrink: the preferred peers that the flat
        // store's own selection would serve lead, and the rest of the
        // selection follows in the flat store's own nearest-first
        // order. The resulting list holds exactly the selected peers.
        let mut preferred_peers: Vec<RouterHash> = Vec::new();
        for key in preferred.iter().copied() {
            if !selected.contains(&key) || preferred_peers.contains(&key) {
                continue;
            }
            preferred_peers.push(key);
        }
        let mut ordered: Vec<RouterHash> = preferred_peers.clone();
        for key in selected.iter().copied() {
            if ordered.contains(&key) {
                continue;
            }
            ordered.push(key);
        }
        state.preferred = Some(preferred_peers);
        let result = self.advance_attempt(state, &ordered);
        self.handle_start_outcome(result, lookup_id)
    }

    fn handle_start_outcome(
        &mut self,
        result: AdvanceOutcome,
        lookup_id: LookupId,
    ) -> StartOutcome {
        match result {
            AdvanceOutcome::Attempt(peer, message) => {
                StartOutcome::PendingAttempt(LookupAction::SendDatabaselookup {
                    lookup_id,
                    peer,
                    message,
                })
            }
            AdvanceOutcome::NeedsPath => {
                StartOutcome::NeedsReplyPath(LookupAction::NeedExploratoryReplyPath { lookup_id })
            }
            AdvanceOutcome::Terminal(reason) => {
                let final_state = reason;
                let lookup = self.active.take().expect("terminal implies active lookup");
                self.active_request_id = None;
                StartOutcome::Terminal(Box::new(LookupResult::Failure {
                    lookup_id,
                    final_state,
                    diagnostics: lookup.diagnostics(),
                }))
            }
        }
    }

    /// Cancels the active lookup, if any. Returns the resulting
    /// terminal action when a lookup was active.
    pub fn cancel(&mut self) -> Option<LookupAction> {
        let lookup = self.active.take()?;
        let lookup_id = LookupId::new(
            self.active_request_id
                .take()
                .unwrap_or_else(|| panic!("active request id must be present")),
            lookup.kind,
            lookup.target,
        );
        let _ = lookup_id; // silence unused
        None
    }

    /// Accepts a reply path for the active lookup. Returns `true`
    /// when the path was stored and the lookup is still active.
    /// Returns `false` when no lookup is active, the supplied
    /// identity does not match, or the reply path is invalid.
    ///
    /// Plan 107 §3.6 exposes this method so the runtime adapter
    /// can convert a `NeedExploratoryReplyPath` action into a real
    /// `SendDatabaselookup` action when the exploratory pool can
    /// supply a `ReplyPath` token.
    pub fn accept_reply_path(&mut self, lookup_id: LookupId, path: ReplyPath) -> bool {
        let active_id = match self.active_lookup() {
            Some(id) => id,
            None => return false,
        };
        if active_id != lookup_id {
            return false;
        }
        let state = match self.active.as_mut() {
            Some(state) => state,
            None => return false,
        };
        state.reply_path = Some(path);
        true
    }

    /// Advance the active lookup to the next action after the reply
    /// path has been supplied. Plan 117 §7 requires the lookup
    /// state machine to immediately emit the next
    /// `SendDatabaselookup` action when the reply path is
    /// available so the daemon seam can drive the active lookup
    /// without reaching into private fields.
    pub fn handle_pending_after_path(
        &mut self,
        store: &RouterInfoStore,
        routing_key: &RouterHash,
    ) -> StartOutcome {
        self.advance_active(store, routing_key)
    }

    /// Advances the active lookup after one emitted attempt went
    /// unanswered within its window. Peers already queried for this
    /// lookup are skipped by the shared advance path, so a retry
    /// rotates to the next eligible floodfill instead of re-asking
    /// the silent one; exhaustion terminates honestly. Fails closed
    /// with no active lookup. Bounded by the same queried/excluded
    /// bookkeeping as every other advance: no new allocations
    /// beyond the existing per-lookup vectors.
    pub fn advance_after_attempt_timeout(
        &mut self,
        store: &RouterInfoStore,
        routing_key: &RouterHash,
    ) -> StartOutcome {
        self.advance_active(store, routing_key)
    }

    /// Plan 489: advances the active lookup after one attempt was
    /// **answered**.
    ///
    /// An answered attempt is not a failure, but the lookup still has
    /// to move on: the next candidate is the one the state machine has
    /// not queried yet. The behaviour is identical to
    /// [`Self::advance_after_attempt_timeout`] — same budget, same
    /// duplicate coalescing, same fail-closed terminal states — and
    /// exists so the caller can name the transition honestly.
    pub fn advance_after_reply(
        &mut self,
        store: &RouterInfoStore,
        routing_key: &RouterHash,
    ) -> StartOutcome {
        self.advance_active(store, routing_key)
    }

    /// Shared body of [`Self::handle_pending_after_path`] and
    /// [`Self::advance_after_attempt_timeout`]: re-selects against
    /// the authoritative store and emits the next unqueried
    /// candidate for the same active lookup.
    fn advance_active(
        &mut self,
        store: &RouterInfoStore,
        routing_key: &RouterHash,
    ) -> StartOutcome {
        let active_id = match self.active_lookup() {
            Some(id) => id,
            None => {
                return StartOutcome::Terminal(Box::new(LookupResult::Failure {
                    lookup_id: LookupId::new(
                        0,
                        LookupKind::RouterInfo,
                        RouterHash::from_bytes([0; 32]),
                    ),
                    final_state: LookupFinalState::Cancelled,
                    diagnostics: LookupDiagnostics::default(),
                }));
            }
        };
        let state = match self.active.take() {
            Some(state) => state,
            None => {
                return StartOutcome::Terminal(Box::new(LookupResult::Failure {
                    lookup_id: active_id,
                    final_state: LookupFinalState::Cancelled,
                    diagnostics: LookupDiagnostics::default(),
                }));
            }
        };
        if state.reply_path.is_none() {
            self.active = Some(state);
            return StartOutcome::NeedsReplyPath(LookupAction::NeedExploratoryReplyPath {
                lookup_id: active_id,
            });
        }
        // Plan 489: a lookup that was started with an explicit
        // candidate list keeps drawing from that list. The list is
        // already ordered closest-first by the routing table, so the
        // state machine never re-sorts it and never consults the flat
        // store behind the caller's back.
        //
        // Plan 497: the routing table's preferred peers are a
        // *priority prefix*, not a closed set. The flat store's current
        // selection still follows them, so a peer ingested while the
        // lookup runs is still queried and no candidate the flat store
        // could serve is lost.
        let candidates: Vec<RouterHash> = match (&state.pinned, &state.preferred) {
            (Some(pinned), _) => pinned.clone(),
            (None, preferred) => {
                let selection = crate::lookup_policy::select_floodfill_candidates(
                    store,
                    &state.target,
                    routing_key,
                    &state.excluded,
                    &self.policy,
                );
                match preferred {
                    Some(preferred) => {
                        let mut ordered: Vec<RouterHash> =
                            Vec::with_capacity(preferred.len() + selection.len());
                        for key in preferred.iter().copied() {
                            // The flat store stays the authority: only a peer
                            // the store still holds as a floodfill advertiser may
                            // lead the list.
                            if !store
                                .get(&key)
                                .is_some_and(|record| record.advertises_floodfill())
                            {
                                continue;
                            }
                            if ordered.contains(&key) {
                                continue;
                            }
                            ordered.push(key);
                        }
                        for key in selection.hashes() {
                            if ordered.contains(&key) {
                                continue;
                            }
                            ordered.push(key);
                        }
                        ordered
                    }
                    None => selection.hashes(),
                }
            }
        };
        let outcome = self.advance_attempt(state, &candidates);
        self.handle_start_outcome(outcome, active_id)
    }

    /// Returns the current active lookup's diagnostics, if any.
    pub fn diagnostics(&self) -> Option<LookupDiagnostics> {
        self.active.as_ref().map(|state| state.diagnostics())
    }

    /// Test-only seam that pre-seeds the active lookup state so
    /// composition tests in adjacent crates can drive the response
    /// ingestion path without going through the full state-machine
    /// start. Production callers must continue to drive the state
    /// machine through `start` / `accept_reply_path` /
    /// `handle_pending_after_path`.
    #[doc(hidden)]
    pub fn seed_active_with_reply_path_for_test(
        &mut self,
        lookup_id: LookupId,
        reply_gateway: RouterHash,
        reply_tunnel_id: u32,
    ) {
        self.active_request_id = Some(lookup_id.request_id());
        self.active = Some(ActiveLookup::new_with_reply_path(
            lookup_id,
            self.policy.max_suggested_hashes(),
            reply_gateway,
            reply_tunnel_id,
        ));
    }

    /// Test-only accessor that returns the active lookup state.
    #[doc(hidden)]
    pub fn active_for_test(&self) -> Option<&ActiveLookup> {
        self.active.as_ref()
    }
}

// Internal helpers --------------------------------------------------

enum AdvanceOutcome {
    Attempt(RouterHash, DatabaseLookupMessage),
    NeedsPath,
    Terminal(LookupFinalState),
}

impl RouterInfoLookup {
    fn advance_attempt(
        &mut self,
        mut state: ActiveLookup,
        candidates: &[RouterHash],
    ) -> AdvanceOutcome {
        // Pick the next candidate that has not yet been queried or
        // excluded. The state machine does not record `queried`
        // bookkeeping until we actually emit an attempt; if the
        // reply path is missing, we preserve the active state and
        // return without consuming a candidate.
        for &key in candidates {
            if state.queried.contains(&key) || state.excluded.contains(&key) {
                continue;
            }
            let path = match state.reply_path {
                Some(path) => path,
                None => {
                    // Store state for the next call without
                    // advancing the queried counter.
                    self.active = Some(state);
                    return AdvanceOutcome::NeedsPath;
                }
            };
            // Plan 500: the policy's per-lookup peer budget is a hard
            // ceiling on the number of attempts. The check sits *before*
            // the peer is pushed onto `queried` and *before* the attempt
            // counter moves, so the budget narrows **attempts** and
            // never **candidates**: a candidate the budget prevented
            // querying stays unqueried, and the candidate list itself is
            // untouched.
            //
            // `state.attempts` and `state.queried` move together — the
            // peer is pushed onto `queried` in the same breath that the
            // counter is incremented, and nothing else touches either —
            // so the counter is the authoritative figure for the
            // attempts already made.
            debug_assert_eq!(
                state.attempts,
                state.queried.len(),
                "the attempt counter and the queried peers must stay in step"
            );
            if state.attempts >= self.policy.max_peers_per_lookup() {
                self.active = Some(state);
                return AdvanceOutcome::Terminal(LookupFinalState::PeerBudgetExhausted);
            }
            state.queried.push(key);
            state.attempts += 1;
            let excluded: Vec<RouterHash> = state
                .excluded
                .iter()
                .copied()
                .chain(state.queried.iter().copied())
                .filter(|hash| *hash != key)
                .collect();
            let message =
                match build_databaselookup(&state.target, state.kind, Some(&path), &excluded) {
                    Ok(message) => message,
                    Err(_) => {
                        // The body builder refused the request. We do
                        // not advance to the next peer because the
                        // exclusion budget is exhausted — bail closed.
                        self.active = Some(state);
                        return AdvanceOutcome::Terminal(LookupFinalState::PeerExhausted);
                    }
                };
            self.active = Some(state);
            return AdvanceOutcome::Attempt(key, message);
        }
        // No eligible candidate remains.
        let queried_is_empty = state.queried.is_empty();
        self.active = Some(state);
        if queried_is_empty {
            AdvanceOutcome::Terminal(LookupFinalState::NoEligibleCandidates)
        } else {
            AdvanceOutcome::Terminal(LookupFinalState::PeerExhausted)
        }
    }
}

/// Outcome of ingesting a single response into a `RouterInfoLookup`.
///
/// The runtime adapter builds the outcome from the I2NP body and
/// hands it back to the state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResponseOutcome {
    /// The lookup completed with a validated `RouterInfo`. The
    /// runtime should hand the validated record to the store.
    Completed(Box<LookupResult>),
    /// The lookup remains active and may still emit another attempt.
    Continue,
    /// The lookup was already terminal; the response was ignored.
    Ignored,
}

/// Ingest a `DatabaseStore` response. The state machine validates
/// the payload against the lookup identity and the plan 103
/// validator.
pub fn handle_database_store(
    lookup: &mut RouterInfoLookup,
    store: &mut RouterInfoStore,
    lookup_id: LookupId,
    store_message: &DatabaseStoreMessage,
    context: ValidationContext,
) -> Result<ResponseOutcome, LookupEngineError> {
    let active = match lookup.active.as_ref() {
        Some(active) => active,
        None => return Ok(ResponseOutcome::Ignored),
    };
    let active_id = LookupId::new(
        lookup
            .active_request_id
            .ok_or(LookupEngineError::LookupTerminal)?,
        active.kind,
        active.target,
    );
    if active_id != lookup_id {
        return Err(LookupEngineError::IdentityMismatch {
            actual: lookup_id,
            expected: active_id,
        });
    }
    if active.kind == LookupKind::Exploratory {
        // Plan 489: an exploratory lookup is answered by a
        // `DatabaseSearchReply`, never by a stored record. A
        // `DatabaseStore` that happens to name the random search key
        // must not complete the lookup: the exploration's answer is
        // the peer list, and the peer list is the only thing the
        // exploration job consumes.
        return Ok(ResponseOutcome::Continue);
    }
    let target = active.target;
    let key = RouterHash::from_hash(store_message.key);
    if key != target {
        // An unrelated RouterInfo cannot complete the lookup.
        return Ok(ResponseOutcome::Continue);
    }
    let compressed = match &store_message.data {
        DatabaseStoreData::RouterInfoCompressed(payload) => payload.as_bytes().to_vec(),
        DatabaseStoreData::LeaseSet(_)
        | DatabaseStoreData::LeaseSet2(_)
        | DatabaseStoreData::MetaLeaseSet(_)
        | DatabaseStoreData::EncryptedLeaseSet(_)
        | DatabaseStoreData::Deferred { .. } => {
            return Ok(ResponseOutcome::Continue);
        }
    };
    let decompressed = match decompress_router_info(&compressed) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = error;
            return Ok(ResponseOutcome::Continue);
        }
    };
    if decompressed.len() > MAX_DECOMPRESSED_ROUTER_INFO_BYTES {
        return Ok(ResponseOutcome::Continue);
    }
    let router_info = match RouterInfo::decode(&decompressed, decompressed.len()) {
        Ok(info) => info,
        Err(_) => return Ok(ResponseOutcome::Continue),
    };
    let derived_key = match router_hash(router_info.router_identity()) {
        Ok(key) => key,
        Err(_) => return Ok(ResponseOutcome::Continue),
    };
    if derived_key != target {
        return Ok(ResponseOutcome::Continue);
    }
    let validated = match ValidatedRouterInfo::from_router_info(router_info, Some(target), context)
    {
        Ok(validated) => validated,
        Err(error) => {
            let _ = error;
            return Ok(ResponseOutcome::Continue);
        }
    };
    let outcome = store.insert(validated.clone());
    let _ = outcome;
    let result = LookupResult::Success {
        lookup_id,
        router_info: Box::new(validated),
    };
    lookup.active = None;
    lookup.active_request_id = None;
    Ok(ResponseOutcome::Completed(Box::new(result)))
}

/// Ingest a `DatabaseStore` carrying a Standard `LeaseSet2`. Plan
/// 122 §A extends the lookup state machine so destination lookups
/// produce a `LeaseSet2Success` outcome.
pub fn handle_database_store_lease_set2(
    lookup: &mut RouterInfoLookup,
    store: &mut LeaseSet2Store,
    lookup_id: LookupId,
    store_message: &DatabaseStoreMessage,
    context: LeaseSet2ValidationContext,
) -> Result<ResponseOutcome, LookupEngineError> {
    let active = match lookup.active.as_ref() {
        Some(active) => active,
        None => return Ok(ResponseOutcome::Ignored),
    };
    let active_id = LookupId::new(
        lookup
            .active_request_id
            .ok_or(LookupEngineError::LookupTerminal)?,
        active.kind,
        active.target,
    );
    if active_id != lookup_id {
        return Err(LookupEngineError::IdentityMismatch {
            actual: lookup_id,
            expected: active_id,
        });
    }
    if active.kind != LookupKind::LeaseSet2 {
        // Plan 122 §A: only LeaseSet2 lookups accept LS2 responses.
        return Ok(ResponseOutcome::Continue);
    }
    // The DatabaseStore key for an LS2 lookup is the SHA-256 of the
    // canonical destination encoding. The lookup state machine
    // already tracks the same 32-byte value through `RouterHash`; the
    // LS2 store uses the same bytes via `DestinationHash`. We compare
    // the canonical bytes directly to keep the typed wrappers
    // distinct.
    let target_bytes = active.target.as_bytes();
    if store_message.key.as_bytes() != target_bytes {
        return Ok(ResponseOutcome::Continue);
    }
    let expected = DestinationHash::from_hash(i2pr_proto::Hash::from_bytes(*target_bytes));
    let ls2 = match &store_message.data {
        DatabaseStoreData::LeaseSet2(boxed) => boxed,
        // Plan 351: a type-5 record for a `LeaseSet2`-kind lookup whose key
        // is today's blinded storage key. The key already matched at line
        // 606, which is the only thing this layer can meaningfully check —
        // `ValidatedEncryptedLeaseSet2::validate` needs the blinded key
        // itself, not just its hash, and needs the daily material and any
        // per-client credential to unwrap. Every one of those belongs to the
        // ELS2 owner, and the type-11 profile check is ADR 0032's policy.
        // So this arm performs the bounded size check, keeps the message,
        // and delegates; it deliberately does not validate, decrypt, or
        // install anything.
        DatabaseStoreData::EncryptedLeaseSet(record) => {
            if record
                .encode_to_vec(crate::els2::MAX_ELS2_RECORD_LENGTH)
                .is_err()
            {
                return Ok(ResponseOutcome::Continue);
            }
            // No `store.insert`: the `LeaseSet2Store` is keyed by
            // destination hash and a blinded storage key is not one. The
            // unwrapped record is installed by the owner under the
            // unblinded destination hash it carries.
            lookup.active = None;
            lookup.active_request_id = None;
            return Ok(ResponseOutcome::Completed(Box::new(
                LookupResult::EncryptedLeaseSet2Success {
                    lookup_id,
                    message: Box::new(store_message.clone()),
                },
            )));
        }
        _ => return Ok(ResponseOutcome::Continue),
    };
    let validated =
        match ValidatedLeaseSet2::from_lease_set2(ls2.as_ref().clone(), Some(expected), context) {
            Ok(validated) => validated,
            Err(_) => return Ok(ResponseOutcome::Continue),
        };
    let _ = store.insert(validated.clone());
    let result = LookupResult::LeaseSet2Success {
        lookup_id,
        lease_set2: Box::new(validated),
    };
    lookup.active = None;
    lookup.active_request_id = None;
    Ok(ResponseOutcome::Completed(Box::new(result)))
}

/// Ingest a `DatabaseSearchReply` response. Suggestions are merged
/// into the bounded suggestion buffer.
pub fn handle_search_reply(
    lookup: &mut RouterInfoLookup,
    lookup_id: LookupId,
    reply: &DatabaseSearchReplyMessage,
    max_suggested_hashes: usize,
    suggested_hash_limit: usize,
) -> Result<ResponseOutcome, LookupEngineError> {
    let active = match lookup.active.as_mut() {
        Some(active) => active,
        None => return Ok(ResponseOutcome::Ignored),
    };
    let active_id = LookupId::new(
        lookup
            .active_request_id
            .ok_or(LookupEngineError::LookupTerminal)?,
        active.kind,
        active.target,
    );
    if active_id != lookup_id {
        return Err(LookupEngineError::IdentityMismatch {
            actual: lookup_id,
            expected: active_id,
        });
    }
    let limit = suggested_hash_limit.min(reply.peer_hashes.len());
    for index in 0..limit {
        let peer = RouterHash::from_hash(reply.peer_hashes[index]);
        if active.queried.contains(&peer) || active.excluded.contains(&peer) {
            continue;
        }
        if active.suggestions.contains(&peer) {
            continue;
        }
        if active.suggestions.len() >= max_suggested_hashes {
            break;
        }
        active.suggestions.push(peer);
        active.suggestions_merged += 1;
    }
    Ok(ResponseOutcome::Continue)
}

/// Ingest a typed delivery outcome for a previously-emitted
/// attempt. Failures advance the lookup to the next candidate.
pub fn handle_delivery_outcome(
    lookup: &mut RouterInfoLookup,
    lookup_id: LookupId,
    outcome: DeliveryOutcome,
) -> Result<ResponseOutcome, LookupEngineError> {
    let active = match lookup.active.as_ref() {
        Some(active) => active,
        None => return Ok(ResponseOutcome::Ignored),
    };
    let active_id = LookupId::new(
        lookup
            .active_request_id
            .ok_or(LookupEngineError::LookupTerminal)?,
        active.kind,
        active.target,
    );
    if active_id != lookup_id {
        return Err(LookupEngineError::IdentityMismatch {
            actual: lookup_id,
            expected: active_id,
        });
    }
    match outcome {
        DeliveryOutcome::Delivered => Ok(ResponseOutcome::Continue),
        DeliveryOutcome::TransportFailure | DeliveryOutcome::Timeout => {
            Ok(ResponseOutcome::Continue)
        }
        DeliveryOutcome::DuplicateOrStale => Ok(ResponseOutcome::Continue),
    }
}

/// Categorical delivery outcome reported back to the state machine
/// from the runtime adapter. The mapping is intentionally coarse so
/// the runtime does not leak typed protocol semantics into the
/// state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryOutcome {
    /// The attempt was accepted for delivery.
    Delivered,
    /// The attempt failed at the transport or queue layer.
    TransportFailure,
    /// The attempt deadline elapsed.
    Timeout,
    /// The runtime recognised the attempt as a duplicate or stale.
    DuplicateOrStale,
}

// The state machine carries the active lookup identifier through a
// private field; tests use a public surface instead of touching it
// directly.
impl RouterInfoLookup {
    /// Sets the active request identifier. Used by tests and
    /// adapters that already managed the lookup identity outside.
    pub fn set_active_request_id(&mut self, id: u64) {
        self.active_request_id = Some(id);
    }
}

/// Helper that ingests a complete standard-header `I2npMessage`
/// containing a `DatabaseStore` body and forwards the outcome to the
/// state machine. This is the typed bridge the runtime adapter uses.
pub fn handle_databasestore_message(
    lookup: &mut RouterInfoLookup,
    store: &mut RouterInfoStore,
    lookup_id: LookupId,
    message: &I2npMessage,
    context: ValidationContext,
) -> Result<ResponseOutcome, LookupEngineError> {
    let body = match message.body() {
        I2npBody::DatabaseStore(body) => body,
        _ => return Ok(ResponseOutcome::Ignored),
    };
    handle_database_store(lookup, store, lookup_id, body, context)
}

/// Helper that ingests a complete standard-header `I2npMessage`
/// containing a `DatabaseSearchReply` body and forwards the outcome
/// to the state machine.
pub fn handle_searchreply_message(
    lookup: &mut RouterInfoLookup,
    lookup_id: LookupId,
    message: &I2npMessage,
    policy: &LookupPolicy,
) -> Result<ResponseOutcome, LookupEngineError> {
    let body = match message.body() {
        I2npBody::DatabaseSearchReply(body) => body,
        _ => return Ok(ResponseOutcome::Ignored),
    };
    handle_search_reply(
        lookup,
        lookup_id,
        body,
        policy.max_suggested_hashes(),
        policy.suggested_hash_limit(),
    )
}

/// Pending reader for a coalesced lookup. The Plan 105 design
/// coalesces multiple local requests onto one active network lookup;
/// this helper maintains the per-target waiter list.
#[derive(Default, Debug)]
pub struct CoalescedRouterInfoLookup {
    waiters: BTreeMap<RouterHash, WaiterSet>,
}

impl CoalescedRouterInfoLookup {
    /// Creates an empty coalescing tracker.
    pub const fn new() -> Self {
        Self {
            waiters: BTreeMap::new(),
        }
    }

    /// Registers a local waiter request for the supplied target
    /// hash. Returns `false` if the target already has a full waiter
    /// set.
    pub fn add_waiter(&mut self, target: RouterHash, request_id: u64) -> bool {
        let entry = self.waiters.entry(target).or_default();
        entry.add(request_id)
    }

    /// Removes a waiter. Returns whether the waiter's request_id was
    /// found in the tracker.
    pub fn remove_waiter(&mut self, target: &RouterHash, request_id: u64) -> bool {
        let mut removed = false;
        if let Some(set) = self.waiters.get_mut(target) {
            removed = set.remove(request_id);
            if set.is_empty() {
                self.waiters.remove(target);
            }
        }
        removed
    }

    /// Returns the number of distinct coalesced targets.
    pub fn target_count(&self) -> usize {
        self.waiters.len()
    }

    /// Returns the waiter count for one target.
    pub fn waiter_count(&self, target: &RouterHash) -> usize {
        self.waiters.get(target).map_or(0, WaiterSet::len)
    }
}

// Add a tiny helper for tests to access the validated reference.
#[cfg(test)]
trait ValidatedReference {
    fn validated_reference(&self) -> &i2pr_proto::RouterInfo;
}

#[cfg(test)]
impl ValidatedReference for ValidatedRouterInfo {
    fn validated_reference(&self) -> &i2pr_proto::RouterInfo {
        self.router_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_info::ValidationContext;
    use i2pr_crypto::{ROUTER_SIGNING_KEY_TYPE, RouterIdentityBundle};
    use i2pr_proto::{Date, Mapping};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate(b: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let info = b
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                i2pr_proto::Mapping::empty(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    fn floodfill(b: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let mut options = i2pr_proto::Mapping::builder();
        options.insert("caps".to_owned(), "f".to_owned()).unwrap();
        let info = b
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                options.build().unwrap(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    #[test]
    fn start_without_reply_path_emits_needs_path_action() {
        let mut store = RouterInfoStore::default();
        let signer = bundle(0x601);
        store.insert(floodfill(&signer, 1));
        let target = RouterHash::from_bytes([0x99u8; 32]);
        let lookup_id = LookupId::new(1, LookupKind::RouterInfo, target);
        let policy = LookupPolicy::default();
        let mut lookup = RouterInfoLookup::new(policy);
        let routing_key = RouterHash::from_bytes([0x11u8; 32]);
        let outcome = lookup.start(&store, lookup_id, &routing_key);
        let action = match outcome {
            StartOutcome::NeedsReplyPath(action) => action,
            other => panic!("expected NeedsReplyPath, got {other:?}"),
        };
        assert_eq!(action.lookup_id(), lookup_id);
        assert!(lookup.active_lookup().is_some());
    }

    #[test]
    fn send_action_carries_exact_lookup_target_and_body() {
        let mut store = RouterInfoStore::default();
        let signer = bundle(0x603);
        store.insert(floodfill(&signer, 1));
        let target = RouterHash::from_bytes([0xC0u8; 32]);
        let gateway = RouterHash::from_bytes([0x77u8; 32]);
        let lookup_id = LookupId::new(0xC1, LookupKind::RouterInfo, target);
        let path = ReplyPath::new(gateway, 0xCAFE).expect("path");
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        let routing_key = RouterHash::from_bytes([0x11u8; 32]);
        let outcome = lookup.start(&store, lookup_id, &routing_key);
        let _action = match outcome {
            StartOutcome::NeedsReplyPath(action) => action,
            other => panic!("expected NeedsReplyPath, got {other:?}"),
        };
        assert!(lookup.accept_reply_path(lookup_id, path));
        let action = match lookup.handle_pending_after_path(&store, &routing_key) {
            StartOutcome::PendingAttempt(action) => action,
            other => panic!("expected PendingAttempt, got {other:?}"),
        };
        let LookupAction::SendDatabaselookup { message, peer, .. } = action else {
            panic!("expected SendDatabaselookup");
        };
        let expected_peer = router_hash(signer.identity()).expect("peer hash");
        assert_eq!(peer, expected_peer);
        assert_eq!(
            message.key,
            i2pr_proto::Hash::from_bytes(*target.as_bytes())
        );
        assert_eq!(
            message.from,
            i2pr_proto::Hash::from_bytes(*gateway.as_bytes())
        );
        assert!(message.delivery_flag);
        assert_eq!(message.reply_tunnel_id, Some(0xCAFE));
        assert_eq!(message.lookup_type, LookupKind::RouterInfo.wire_code());
    }

    #[test]
    fn successful_databasestore_completes_lookup() {
        let mut store = RouterInfoStore::default();
        let target_signer = bundle(0x610);
        let target_validated = validate(&target_signer, 1);
        let target = target_validated.key();
        let signer = bundle(0x611);
        store.insert(floodfill(&signer, 1));
        let lookup_id = LookupId::new(7, LookupKind::RouterInfo, target);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        lookup.set_active_request_id(7);
        lookup.active = Some(ActiveLookup::new(
            lookup_id,
            LookupPolicy::default().max_suggested_hashes(),
        ));
        lookup.active.as_mut().unwrap().reply_path =
            Some(ReplyPath::new(RouterHash::from_bytes([0x22u8; 32]), 5).expect("path"));
        let encoded = target_validated
            .validated_reference()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        // gzip the encoded payload.
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        encoder.write_all(&encoded).unwrap();
        let compressed = encoder.finish().unwrap();
        let compressed_len = compressed.len();
        let store_message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        };
        let context = ValidationContext::new(Date::from_millis(1));
        let outcome =
            handle_database_store(&mut lookup, &mut store, lookup_id, &store_message, context)
                .expect("handle");
        match outcome {
            ResponseOutcome::Completed(result) => match *result {
                LookupResult::Success { lookup_id: id, .. } => {
                    assert_eq!(id, lookup_id);
                }
                _ => panic!("expected Success, got {result:?}"),
            },
            other => panic!("expected Completed, got {other:?}"),
        }
        assert!(lookup.active_lookup().is_none());
    }

    #[test]
    fn unrelated_databasestore_does_not_complete_lookup() {
        let mut store = RouterInfoStore::default();
        let target_signer = bundle(0x620);
        let target = router_hash(target_signer.identity()).unwrap();
        let other_signer = bundle(0x621);
        let other = validate(&other_signer, 1);
        let lookup_id = LookupId::new(8, LookupKind::RouterInfo, target);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        lookup.set_active_request_id(8);
        lookup.active = Some(ActiveLookup::new(
            lookup_id,
            LookupPolicy::default().max_suggested_hashes(),
        ));
        lookup.active.as_mut().unwrap().reply_path =
            Some(ReplyPath::new(RouterHash::from_bytes([0x22u8; 32]), 5).expect("path"));
        let encoded = other
            .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        encoder.write_all(&encoded).unwrap();
        let compressed = encoder.finish().unwrap();
        let compressed_len = compressed.len();
        let store_message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*other.key().as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        };
        let context = ValidationContext::new(Date::from_millis(1));
        let outcome =
            handle_database_store(&mut lookup, &mut store, lookup_id, &store_message, context)
                .expect("handle");
        assert_eq!(outcome, ResponseOutcome::Continue);
        assert!(lookup.active_lookup().is_some());
    }

    #[test]
    fn search_reply_merges_bounded_suggestions() {
        let target = RouterHash::from_bytes([0x55u8; 32]);
        let lookup_id = LookupId::new(11, LookupKind::RouterInfo, target);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        lookup.set_active_request_id(11);
        lookup.active = Some(ActiveLookup::new(
            lookup_id,
            LookupPolicy::default().max_suggested_hashes(),
        ));
        let reply = DatabaseSearchReplyMessage {
            key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
            peer_hashes: vec![
                i2pr_proto::Hash::from_bytes([0x01u8; 32]),
                i2pr_proto::Hash::from_bytes([0x02u8; 32]),
                i2pr_proto::Hash::from_bytes([0x03u8; 32]),
            ],
            from: i2pr_proto::Hash::from_bytes([0x04u8; 32]),
        };
        let policy = LookupPolicy::default();
        let outcome = handle_search_reply(
            &mut lookup,
            lookup_id,
            &reply,
            policy.max_suggested_hashes(),
            policy.suggested_hash_limit(),
        )
        .expect("handle");
        assert_eq!(outcome, ResponseOutcome::Continue);
        let active = lookup.active.as_ref().expect("active");
        assert_eq!(active.suggestions.len(), 3);
        assert_eq!(active.suggestions_merged, 3);
    }

    #[test]
    fn duplicate_suggestions_are_deduplicated() {
        let target = RouterHash::from_bytes([0x55u8; 32]);
        let lookup_id = LookupId::new(12, LookupKind::RouterInfo, target);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        lookup.set_active_request_id(12);
        lookup.active = Some(ActiveLookup::new(
            lookup_id,
            LookupPolicy::default().max_suggested_hashes(),
        ));
        let reply = DatabaseSearchReplyMessage {
            key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
            peer_hashes: vec![
                i2pr_proto::Hash::from_bytes([0x01u8; 32]),
                i2pr_proto::Hash::from_bytes([0x01u8; 32]),
            ],
            from: i2pr_proto::Hash::from_bytes([0x04u8; 32]),
        };
        let policy = LookupPolicy::default();
        let outcome = handle_search_reply(
            &mut lookup,
            lookup_id,
            &reply,
            policy.max_suggested_hashes(),
            policy.suggested_hash_limit(),
        )
        .expect("handle");
        assert_eq!(outcome, ResponseOutcome::Continue);
        let active = lookup.active.as_ref().expect("active");
        assert_eq!(active.suggestions.len(), 1);
    }

    #[test]
    fn identity_mismatch_is_rejected() {
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        lookup.set_active_request_id(1);
        lookup.active = Some(ActiveLookup::new(
            LookupId::new(
                1,
                LookupKind::RouterInfo,
                RouterHash::from_bytes([0x55u8; 32]),
            ),
            LookupPolicy::default().max_suggested_hashes(),
        ));
        let bogus = LookupId::new(
            2,
            LookupKind::RouterInfo,
            RouterHash::from_bytes([0x66u8; 32]),
        );
        let store_message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes([0u8; 32]),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(Vec::new(), 0).expect("payload"),
            ),
        };
        let context = ValidationContext::new(Date::from_millis(0));
        let error = handle_database_store(
            &mut lookup,
            &mut RouterInfoStore::default(),
            bogus,
            &store_message,
            context,
        )
        .unwrap_err();
        assert!(matches!(error, LookupEngineError::IdentityMismatch { .. }));
    }

    #[test]
    fn coalesced_tracker_dedupes_targets_and_tracks_waiters() {
        let mut tracker = CoalescedRouterInfoLookup::new();
        let target_a = RouterHash::from_bytes([0x01u8; 32]);
        let target_b = RouterHash::from_bytes([0x02u8; 32]);
        assert!(tracker.add_waiter(target_a, 1));
        assert!(tracker.add_waiter(target_a, 2));
        assert!(!tracker.add_waiter(target_a, 1)); // duplicate rejected
        assert!(tracker.add_waiter(target_b, 3));
        assert_eq!(tracker.target_count(), 2);
        assert_eq!(tracker.waiter_count(&target_a), 2);
        assert!(tracker.remove_waiter(&target_a, 1));
        assert_eq!(tracker.target_count(), 2);
        assert!(tracker.remove_waiter(&target_a, 2));
        assert_eq!(tracker.target_count(), 1);
    }

    #[test]
    fn invalid_signature_routerinfo_does_not_complete_lookup() {
        let signer = bundle(0x900);
        let target_signer = bundle(0x901);
        let target = router_hash(target_signer.identity()).unwrap();
        let lookup_id = LookupId::new(20, LookupKind::RouterInfo, target);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        lookup.set_active_request_id(20);
        lookup.active = Some(ActiveLookup::new(
            lookup_id,
            LookupPolicy::default().max_suggested_hashes(),
        ));
        lookup.active.as_mut().unwrap().reply_path =
            Some(ReplyPath::new(RouterHash::from_bytes([0x22u8; 32]), 5).expect("path"));
        // Tamper with the signature bytes of an otherwise-valid
        // RouterInfo. The validator must reject the result and the
        // lookup must remain active.
        let options = Mapping::empty();
        let info = target_signer
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                options.clone(),
            )
            .expect("sign");
        let bad_signature = {
            let mut bytes = info.signature().as_bytes().to_vec();
            bytes[0] ^= 0x01;
            i2pr_proto::SignatureValue::new(ROUTER_SIGNING_KEY_TYPE, bytes).expect("signature")
        };
        let tampered_info = i2pr_proto::RouterInfo::new(
            info.router_identity().clone(),
            info.published(),
            info.addresses().to_vec(),
            Vec::new(),
            options,
            bad_signature,
        )
        .expect("tampered info");
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer, 1));
        let encoded = tampered_info
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        encoder.write_all(&encoded).unwrap();
        let compressed = encoder.finish().unwrap();
        let compressed_len = compressed.len();
        let store_message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        };
        let context = ValidationContext::new(Date::from_millis(1));
        let outcome =
            handle_database_store(&mut lookup, &mut store, lookup_id, &store_message, context)
                .expect("handle");
        assert_eq!(outcome, ResponseOutcome::Continue);
        assert!(lookup.active_lookup().is_some());
    }

    #[test]
    fn late_response_does_not_revive_terminal_lookup() {
        let signer = bundle(0x910);
        let target_signer = bundle(0x911);
        let target_validated = validate(&target_signer, 1);
        let target = target_validated.key();
        let mut store = RouterInfoStore::default();
        store.insert(validate(&signer, 1));
        let lookup_id = LookupId::new(21, LookupKind::RouterInfo, target);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        lookup.set_active_request_id(21);
        lookup.active = Some(ActiveLookup::new(
            lookup_id,
            LookupPolicy::default().max_suggested_hashes(),
        ));
        lookup.active.as_mut().unwrap().reply_path =
            Some(ReplyPath::new(RouterHash::from_bytes([0x22u8; 32]), 5).expect("path"));
        let encoded = target_validated
            .validated_reference()
            .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        encoder.write_all(&encoded).unwrap();
        let compressed = encoder.finish().unwrap();
        let compressed_len = compressed.len();
        let store_message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        };
        let context = ValidationContext::new(Date::from_millis(1));
        let outcome =
            handle_database_store(&mut lookup, &mut store, lookup_id, &store_message, context)
                .expect("handle");
        if let ResponseOutcome::Completed(result) = &outcome {
            assert!(matches!(**result, LookupResult::Success { .. }));
        } else {
            panic!("expected Completed, got {outcome:?}");
        }
        // The lookup is now terminal. A late response must be
        // ignored.
        let outcome =
            handle_database_store(&mut lookup, &mut store, lookup_id, &store_message, context)
                .expect("handle");
        assert_eq!(outcome, ResponseOutcome::Ignored);
    }

    #[test]
    fn suggested_hashes_are_bounded_by_policy() {
        let target = RouterHash::from_bytes([0x55u8; 32]);
        let lookup_id = LookupId::new(31, LookupKind::RouterInfo, target);
        let policy = LookupPolicy::new(
            1, 1, // max_suggested_hashes is 4; suggested_hash_limit is 2.
            4, 2, 5_000, 1_500,
        )
        .expect("policy");
        let mut lookup = RouterInfoLookup::new(policy);
        lookup.set_active_request_id(31);
        lookup.active = Some(ActiveLookup::new(lookup_id, policy.max_suggested_hashes()));
        let reply = DatabaseSearchReplyMessage {
            key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
            peer_hashes: vec![
                i2pr_proto::Hash::from_bytes([0x01u8; 32]),
                i2pr_proto::Hash::from_bytes([0x02u8; 32]),
                i2pr_proto::Hash::from_bytes([0x03u8; 32]),
                i2pr_proto::Hash::from_bytes([0x04u8; 32]),
                i2pr_proto::Hash::from_bytes([0x05u8; 32]),
            ],
            from: i2pr_proto::Hash::from_bytes([0x06u8; 32]),
        };
        let outcome = handle_search_reply(
            &mut lookup,
            lookup_id,
            &reply,
            policy.max_suggested_hashes(),
            policy.suggested_hash_limit(),
        )
        .expect("handle");
        assert_eq!(outcome, ResponseOutcome::Continue);
        // First reply contributes at most `suggested_hash_limit`
        // suggestions. Send a second reply to fill the remaining
        // budget up to `max_suggested_hashes`.
        let reply_two = DatabaseSearchReplyMessage {
            key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
            peer_hashes: vec![
                i2pr_proto::Hash::from_bytes([0x06u8; 32]),
                i2pr_proto::Hash::from_bytes([0x07u8; 32]),
            ],
            from: i2pr_proto::Hash::from_bytes([0x08u8; 32]),
        };
        let outcome = handle_search_reply(
            &mut lookup,
            lookup_id,
            &reply_two,
            policy.max_suggested_hashes(),
            policy.suggested_hash_limit(),
        )
        .expect("handle");
        assert_eq!(outcome, ResponseOutcome::Continue);
        let active = lookup.active.as_ref().expect("active");
        assert_eq!(active.suggestions.len(), policy.max_suggested_hashes());
    }

    #[test]
    fn delivery_failure_is_recorded_without_reviving() {
        let target = RouterHash::from_bytes([0x55u8; 32]);
        let lookup_id = LookupId::new(32, LookupKind::RouterInfo, target);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        lookup.set_active_request_id(32);
        lookup.active = Some(ActiveLookup::new(
            lookup_id,
            LookupPolicy::default().max_suggested_hashes(),
        ));
        let outcome = handle_delivery_outcome(
            &mut lookup,
            lookup_id,
            crate::lookup_engine::DeliveryOutcome::TransportFailure,
        )
        .expect("handle");
        assert_eq!(outcome, ResponseOutcome::Continue);
        let active = lookup.active.as_ref().expect("active");
        assert_eq!(active.diagnostics().attempts, 0);
    }

    #[test]
    fn duplicate_waiters_are_coalesced() {
        let target = RouterHash::from_bytes([0x99u8; 32]);
        let mut coalesced = CoalescedRouterInfoLookup::new();
        assert!(coalesced.add_waiter(target, 1));
        assert!(coalesced.add_waiter(target, 2));
        assert!(!coalesced.add_waiter(target, 1));
        assert_eq!(coalesced.waiter_count(&target), 2);
        assert!(coalesced.remove_waiter(&target, 2));
        assert_eq!(coalesced.waiter_count(&target), 1);
        assert!(coalesced.remove_waiter(&target, 1));
        assert_eq!(coalesced.target_count(), 0);
    }

    // Plan 497 — the preferred-order path --------------------------

    /// Drives a lookup to its first attempt and returns the peer it
    /// asks.
    fn first_attempt_peer(
        lookup: &mut RouterInfoLookup,
        store: &RouterInfoStore,
        lookup_id: LookupId,
        routing_key: &RouterHash,
    ) -> RouterHash {
        let path = ReplyPath::new(RouterHash::from_bytes([0x77u8; 32]), 0xCAFE).expect("path");
        assert!(lookup.accept_reply_path(lookup_id, path));
        let action = match lookup.handle_pending_after_path(store, routing_key) {
            StartOutcome::PendingAttempt(action) => action,
            other => panic!("expected PendingAttempt, got {other:?}"),
        };
        let LookupAction::SendDatabaselookup { peer, .. } = action else {
            panic!("expected SendDatabaselookup");
        };
        peer
    }

    #[test]
    fn a_preferred_peer_is_asked_first() {
        let mut store = RouterInfoStore::default();
        let first = bundle(0x701);
        let second = bundle(0x702);
        store.insert(floodfill(&first, 1));
        store.insert(floodfill(&second, 1));
        let first_key = router_hash(first.identity()).expect("hash");
        let target = RouterHash::from_bytes([0xC0u8; 32]);
        let lookup_id = LookupId::new(0x71, LookupKind::RouterInfo, target);
        let routing_key = RouterHash::from_bytes([0x11u8; 32]);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        let outcome = lookup.start_with_preferred(&store, lookup_id, &routing_key, &[first_key]);
        assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
        let peer = first_attempt_peer(&mut lookup, &store, lookup_id, &routing_key);
        assert_eq!(
            peer, first_key,
            "the preferred peer must be asked before the flat store's order"
        );
    }

    #[test]
    fn a_preferred_peer_the_flat_store_would_not_serve_is_ignored() {
        // The flat store is the authority: the preference may reorder
        // the selection but never widen it.
        let mut store = RouterInfoStore::default();
        let known = bundle(0x703);
        store.insert(floodfill(&known, 1));
        let known_key = router_hash(known.identity()).expect("hash");
        let unknown = bundle(0x704);
        let unknown_key = router_hash(unknown.identity()).expect("hash");
        let target = RouterHash::from_bytes([0xC0u8; 32]);
        let lookup_id = LookupId::new(0x72, LookupKind::RouterInfo, target);
        let routing_key = RouterHash::from_bytes([0x11u8; 32]);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        let outcome = lookup.start_with_preferred(&store, lookup_id, &routing_key, &[unknown_key]);
        assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
        let peer = first_attempt_peer(&mut lookup, &store, lookup_id, &routing_key);
        assert_eq!(
            peer, known_key,
            "a peer the flat store does not hold must never be asked"
        );
    }

    #[test]
    fn the_preferred_path_never_asks_the_target() {
        let mut store = RouterInfoStore::default();
        let target_signer = bundle(0x705);
        let other = bundle(0x706);
        store.insert(floodfill(&target_signer, 1));
        store.insert(floodfill(&other, 1));
        let target = router_hash(target_signer.identity()).expect("hash");
        let other_key = router_hash(other.identity()).expect("hash");
        let lookup_id = LookupId::new(0x73, LookupKind::RouterInfo, target);
        let routing_key = RouterHash::from_bytes([0x11u8; 32]);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        let outcome = lookup.start_with_preferred(&store, lookup_id, &routing_key, &[target]);
        assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
        let peer = first_attempt_peer(&mut lookup, &store, lookup_id, &routing_key);
        assert_eq!(
            peer, other_key,
            "the lookup target must stay excluded even when preferred"
        );
    }

    #[test]
    fn the_preferred_path_still_reaches_a_peer_ingested_mid_lookup() {
        // Plan 497's no-regression row: the preferred order is a
        // priority prefix, not a closed set, so a peer learned while
        // the lookup runs is still queried.
        let mut store = RouterInfoStore::default();
        let first = bundle(0x707);
        store.insert(floodfill(&first, 1));
        let first_key = router_hash(first.identity()).expect("hash");
        let target = RouterHash::from_bytes([0xC0u8; 32]);
        let lookup_id = LookupId::new(0x74, LookupKind::RouterInfo, target);
        let routing_key = RouterHash::from_bytes([0x11u8; 32]);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        let outcome = lookup.start_with_preferred(&store, lookup_id, &routing_key, &[first_key]);
        assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
        let peer = first_attempt_peer(&mut lookup, &store, lookup_id, &routing_key);
        assert_eq!(peer, first_key);

        // A second floodfill arrives in the store while the lookup runs.
        let second = bundle(0x708);
        let second_key = router_hash(second.identity()).expect("hash");
        store.insert(floodfill(&second, 1));

        let action = match lookup.advance_after_reply(&store, &routing_key) {
            StartOutcome::PendingAttempt(action) => action,
            other => panic!("expected PendingAttempt, got {other:?}"),
        };
        let LookupAction::SendDatabaselookup { peer, .. } = action else {
            panic!("expected SendDatabaselookup");
        };
        assert_eq!(
            peer, second_key,
            "a peer ingested during the lookup must still be queried"
        );
    }

    #[test]
    fn the_closed_candidate_list_does_not_reach_a_peer_ingested_mid_lookup() {
        // The control for the row above: Plan 489's closed list keeps
        // its documented behaviour, which is why Plan 497 needs its own
        // entry point rather than reusing this one.
        let mut store = RouterInfoStore::default();
        let first = bundle(0x709);
        store.insert(floodfill(&first, 1));
        let first_key = router_hash(first.identity()).expect("hash");
        let target = RouterHash::from_bytes([0xC0u8; 32]);
        let lookup_id = LookupId::new(0x75, LookupKind::RouterInfo, target);
        let routing_key = RouterHash::from_bytes([0x11u8; 32]);
        let mut lookup = RouterInfoLookup::new(LookupPolicy::default());
        let outcome = lookup.start_with_candidates(lookup_id, &[first_key]);
        assert!(matches!(outcome, StartOutcome::NeedsReplyPath(_)));
        let peer = first_attempt_peer(&mut lookup, &store, lookup_id, &routing_key);
        assert_eq!(peer, first_key);

        let second = bundle(0x70A);
        store.insert(floodfill(&second, 1));
        let outcome = lookup.advance_after_reply(&store, &routing_key);
        assert!(
            matches!(outcome, StartOutcome::Terminal(_)),
            "the closed list must not consult the store behind the caller's back"
        );
    }
}
