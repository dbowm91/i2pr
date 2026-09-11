//! Plan 186 daemon-owned NetDB-over-exploratory-tunnels coordinator.
//!
//! The coordinator ties the existing NetDB state machines over the
//! real Plan 185 exploratory tunnel pair:
//!
//! ```text
//! i2pr lookup request
//!  -> real inbound exploratory reply path selected
//!  -> real outbound exploratory path selected
//!  -> existing LookupAction::SendDatabaseLookup
//!  -> existing outbound tunnel composition
//!  -> TunnelData over Plan 185 path
//!  -> i2pd floodfill
//!  -> reply routed to i2pr inbound exploratory tunnel
//!  -> TunnelData recovery
//!  -> inbound_dispatch / NetDbSeam
//!  -> signed RouterInfo validation
//!  -> authoritative RouterInfo store
//!  -> terminal lookup success
//! ```
//!
//! Properties:
//!
//! - one bounded authoritative [`RouterInfoStore`] populated only by
//!   validated signed RouterInfo records; the live lookup path never
//!   consults the Plan 122 LeaseSet2 placeholder empty store;
//! - bootstrap traverses the same parser/signature/freshness
//!   validation used for ordinary records before eligibility;
//! - floodfill selection reuses the existing
//!   [`select_floodfill_candidates`] routing-key logic;
//! - outbound dispatch reuses [`compose_outbound_lookup`] /
//!   [`compose_outbound_publication`] through the supplied
//!   [`OutboundGatewayRole`]; direct SSU2 `DatabaseLookup` delivery is
//!   never a counted success path;
//! - inbound recovery reuses [`dispatch_inbound_tunnel_data`] +
//!   [`route_databasestore`] / [`route_database_search_reply`];
//! - publication reuses [`PublicationCoordinator`] with normal i2pr
//!   identity/publication code and protocol-derived acknowledgement;
//! - concurrent lookups, candidate count, pending publication
//!   acknowledgements, retained reply paths, store entries/bytes, and
//!   retries/deadlines are all bounded;
//! - tunnel loss returns a typed retry/failure and never silently
//!   falls back to direct transport.
//!
//! The module is runtime-neutral: it never opens sockets, never spawns
//! tasks, never performs DNS, never touches the filesystem. All I/O
//! happens through the caller-supplied tunnel roles and registry.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use i2pr_netdb::{
    LookupAction, LookupId, LookupPolicy, PublicationAttemptRecord, PublicationCoordinator,
    ReplyPath, ResponseOutcome, RouterHash, RouterInfoStore, RouterInfoStoreConfig,
    ValidationContext, select_floodfill_candidates,
};
use i2pr_proto::{
    Date, Hash, I2npBody, I2npMessage, MAX_COMMON_STRUCTURE_SIZE, MAX_I2NP_PAYLOAD_SIZE,
};
use i2pr_transport::Deadline;
use i2pr_tunnel::data_plane_registry::DataPlaneRegistry;
use i2pr_tunnel::roles::OutboundGatewayRole;
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;

/// Maximum concurrent RouterInfo lookups the coordinator tracks.
pub const MAX_CONCURRENT_LOOKUPS: usize = 8;
/// Maximum retained inbound reply paths.
pub const MAX_RETAINED_REPLY_PATHS: usize = 8;
/// Maximum lookup retries before the coordinator reports peer-exhausted.
pub const MAX_LOOKUP_RETRIES: u32 = 3;
/// Default lookup deadline in milliseconds (matches the lookup policy total).
pub const DEFAULT_LOOKUP_DEADLINE_MS: u64 = 5_000;
/// Starting request identifier.
pub const INITIAL_NETDB_REQUEST_ID: u64 = 1;

/// Typed failures for the NetDB-over-tunnels coordinator.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum NetDbTunnelError {
    /// Too many concurrent lookups are active.
    #[error("too many concurrent NetDB lookups")]
    TooManyLookups,
    /// Too many retained reply paths are held.
    #[error("too many retained NetDB reply paths")]
    TooManyReplyPaths,
    /// No eligible floodfill candidate exists in the authoritative store.
    #[error("no eligible floodfill candidate")]
    NoEligibleCandidates,
    /// The lookup identifier is unknown.
    #[error("unknown NetDB lookup")]
    UnknownLookup,
    /// The supplied reply path is invalid.
    #[error("invalid NetDB reply path")]
    InvalidReplyPath,
    /// The reference RouterInfo bytes were rejected.
    #[error("reference RouterInfo rejected: {0}")]
    ReferenceRejected(String),
    /// The lookup state machine rejected the operation.
    #[error("lookup engine rejected the operation: {0}")]
    LookupEngine(String),
    /// Outbound tunnel composition failed.
    #[error("outbound tunnel composition failed: {0}")]
    Outbound(String),
    /// Inbound tunnel recovery failed.
    #[error("inbound tunnel recovery failed: {0}")]
    Inbound(String),
    /// Publication coordination failed.
    #[error("publication coordination failed: {0}")]
    Publication(String),
    /// Direct-transport NetDB delivery is never a counted success path.
    #[error("direct-transport NetDB delivery is not a counted path")]
    DirectTransportRejected,
    /// The exploratory tunnel was lost during the lookup.
    #[error("exploratory tunnel lost during NetDB lookup")]
    TunnelLost,
    /// The lookup deadline elapsed.
    #[error("NetDB lookup deadline elapsed")]
    DeadlineExceeded,
    /// The recovered envelope was malformed.
    #[error("recovered NetDB envelope is malformed")]
    Malformed,
    /// The lookup was cancelled.
    #[error("NetDB lookup was cancelled")]
    Cancelled,
}

/// Privacy-safe bounded counters for the coordinator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NetDbTunnelCounters {
    /// Lookups started through the authoritative store.
    pub lookups_started: u64,
    /// Lookups that reached terminal validation + install.
    pub lookups_succeeded: u64,
    /// Lookups that terminated without success.
    pub lookups_failed: u64,
    /// Publications started through the existing coordinator.
    pub publications_started: u64,
    /// Publications acknowledged via protocol-derived evidence.
    pub publications_acked: u64,
    /// Search-reply suggestions merged (bounded).
    pub search_replies_merged: u64,
    /// Mismatched key/store responses rejected.
    pub mismatched_rejected: u64,
    /// Stale/invalid RouterInfo responses rejected.
    pub stale_rejected: u64,
    /// Malformed recovered I2NP envelopes rejected.
    pub malformed_rejected: u64,
    /// Duplicate responses ignored.
    pub duplicate_ignored: u64,
    /// Lookup deadlines expired.
    pub timeouts: u64,
    /// Lookups cancelled.
    pub cancellations: u64,
    /// Tunnel-loss failures reported (typed, no fallback).
    pub tunnel_failures: u64,
    /// Direct-transport attempts rejected (never counted).
    pub direct_rejections: u64,
    /// Pending lookups at the snapshot.
    pub pending_lookups: usize,
    /// Retained reply paths at the snapshot.
    pub retained_paths: usize,
}

/// Proof that one outbound lookup traversed the exploratory tunnel path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TunnelPathProof {
    /// Number of TunnelData cells emitted.
    pub cell_count: usize,
    /// Outbound first-hop router (delivery target P).
    pub first_hop: Hash,
    /// Selected floodfill peer (tunnel ROUTER target F).
    pub floodfill: Hash,
    /// DatabaseLookup key (K, may differ from F).
    pub lookup_key: Hash,
    /// Always true; the type exists so callers cannot substitute a
    /// direct-transport send for a counted row.
    pub via_tunnel: bool,
}

impl std::fmt::Display for TunnelPathProof {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "tunnel-path(cells={}, via_tunnel={})",
            self.cell_count, self.via_tunnel
        )
    }
}

/// Proof that one publication traversed the outbound tunnel path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationPathProof {
    /// Number of TunnelData cells emitted.
    pub cell_count: usize,
    /// Outbound first-hop router (delivery target P).
    pub first_hop: Hash,
    /// Selected floodfill peer (tunnel ROUTER target F).
    pub floodfill: Hash,
    /// Always true; direct transport is never a counted path.
    pub via_tunnel: bool,
}

impl std::fmt::Display for PublicationPathProof {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "publication-path(cells={}, via_tunnel={})",
            self.cell_count, self.via_tunnel
        )
    }
}

/// One pending lookup tracked by the coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingLookup {
    lookup_id: LookupId,
    target: RouterHash,
    deadline_ms: u64,
    retries: u32,
}

/// Test-only static reply-path provider. Production callers inject the
/// real inbound tunnel path through [`NetDbTunnelCoordinator`]; the
/// provider never consults transport state.
#[derive(Debug)]
struct StaticReplyProvider {
    path: ReplyPath,
}

impl i2pr_netdb::ReplyPathProvider for StaticReplyProvider {
    fn has_inbound_tunnel(&self) -> bool {
        true
    }
    fn provide_reply_path(&self) -> Option<ReplyPath> {
        Some(self.path)
    }
}

/// Daemon-owned NetDB-over-exploratory-tunnels coordinator.
pub struct NetDbTunnelCoordinator {
    store: RouterInfoStore,
    policy: LookupPolicy,
    seam: crate::netdb_seam::NetDbSeam,
    publication: PublicationCoordinator,
    pending: BTreeMap<u64, PendingLookup>,
    retained_paths: BTreeMap<u64, ReplyPath>,
    next_request_id: u64,
    now_ms: u64,
    counters: NetDbTunnelCounters,
}

impl NetDbTunnelCoordinator {
    /// Constructs a coordinator with the supplied bounded policy and
    /// authoritative store configuration.
    pub fn new(policy: LookupPolicy, store_config: RouterInfoStoreConfig) -> Self {
        Self {
            store: RouterInfoStore::with_config(store_config),
            policy,
            seam: crate::netdb_seam::NetDbSeam::new(policy),
            publication: PublicationCoordinator::new(policy),
            pending: BTreeMap::new(),
            retained_paths: BTreeMap::new(),
            next_request_id: INITIAL_NETDB_REQUEST_ID,
            now_ms: 0,
            counters: NetDbTunnelCounters::default(),
        }
    }

    /// Returns the authoritative bounded store.
    pub fn store(&self) -> &RouterInfoStore {
        &self.store
    }

    /// Returns the bounded lookup policy.
    pub const fn policy(&self) -> LookupPolicy {
        self.policy
    }

    /// Returns the bounded counters snapshot.
    pub fn counters(&self) -> NetDbTunnelCounters {
        let mut snapshot = self.counters;
        snapshot.pending_lookups = self.pending.len();
        snapshot.retained_paths = self.retained_paths.len();
        snapshot
    }

    /// Advances the wall-clock view. Callers must call this before any
    /// operation that depends on time.
    pub fn advance_time(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// Returns the number of pending lookups.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Returns the number of retained reply paths.
    pub fn retained_len(&self) -> usize {
        self.retained_paths.len()
    }

    /// Bootstraps the authoritative store with exact signed RouterInfo
    /// material for the reference router.
    ///
    /// The bytes traverse the same parser/signature/freshness
    /// validation used for ordinary records before becoming eligible.
    /// No public reseed is required or permitted.
    pub fn bootstrap_reference_router_info(
        &mut self,
        bytes: &[u8],
        now: Date,
    ) -> Result<RouterHash, NetDbTunnelError> {
        let info = i2pr_proto::RouterInfo::decode(bytes, MAX_COMMON_STRUCTURE_SIZE)
            .map_err(|e| NetDbTunnelError::ReferenceRejected(format!("{e:?}")))?;
        let context = ValidationContext::new(now);
        let validated = i2pr_netdb::ValidatedRouterInfo::from_router_info(info, None, context)
            .map_err(|e| NetDbTunnelError::ReferenceRejected(e.to_string()))?;
        let key = validated.key();
        let outcome = self.store.insert(validated);
        match outcome {
            i2pr_netdb::InsertOutcome::Inserted
            | i2pr_netdb::InsertOutcome::Replaced
            | i2pr_netdb::InsertOutcome::Idempotent => Ok(key),
            i2pr_netdb::InsertOutcome::Conflict => {
                Err(NetDbTunnelError::ReferenceRejected("conflict".to_owned()))
            }
            i2pr_netdb::InsertOutcome::StaleReplacement => {
                Err(NetDbTunnelError::ReferenceRejected("stale".to_owned()))
            }
            i2pr_netdb::InsertOutcome::CapacityExceeded => {
                Err(NetDbTunnelError::ReferenceRejected("capacity".to_owned()))
            }
        }
    }

    /// Returns whether the supplied hash is a floodfill advertiser in
    /// the authoritative store. The effective RouterInfo capability is
    /// verified before dispatch.
    pub fn is_floodfill(&self, hash: &RouterHash) -> bool {
        self.store
            .get(hash)
            .map(|record| record.advertises_floodfill())
            .unwrap_or(false)
    }

    /// Returns the count of floodfill advertisers in the authoritative store.
    pub fn floodfill_count(&self) -> usize {
        self.store.stats().floodfill_advertiser_count
    }

    /// Returns the bounded floodfill candidate set for the supplied
    /// target/routing key through the authoritative store. The live
    /// path never consults the placeholder empty store.
    pub fn candidate_hashes(
        &self,
        target: &RouterHash,
        routing_key: &RouterHash,
    ) -> Vec<RouterHash> {
        select_floodfill_candidates(&self.store, target, routing_key, &[], &self.policy).hashes()
    }

    /// Returns the typed composition outcome derived from the real
    /// [`DataPlaneRegistry`] at the coordinator's current time.
    pub fn composition_outcome_with_registry(
        &self,
        registry: &DataPlaneRegistry,
    ) -> crate::netdb_seam::CompositionOutcome {
        self.seam
            .composition_outcome_with_registry(registry, self.now_ms)
    }

    /// Begins one RouterInfo lookup through the authoritative bounded
    /// store for real candidate selection.
    ///
    /// The caller supplies the real inbound exploratory reply path
    /// (selected from the Plan 185 registry pair). The seam drives the
    /// existing state machine immediately; the returned action must be
    /// [`LookupAction::SendDatabaselookup`] for a counted row. Terminal
    /// `NoEligibleCandidates` / `PeerExhausted` outcomes terminate
    /// honestly rather than spinning.
    pub fn begin_tunnel_lookup(
        &mut self,
        target: RouterHash,
        routing_key: &RouterHash,
        reply_path: ReplyPath,
    ) -> Result<(u64, LookupAction), NetDbTunnelError> {
        if self.pending.len() >= MAX_CONCURRENT_LOOKUPS {
            return Err(NetDbTunnelError::TooManyLookups);
        }
        if self.retained_paths.len() >= MAX_RETAINED_REPLY_PATHS
            && !self.retained_paths.values().any(|p| *p == reply_path)
        {
            return Err(NetDbTunnelError::TooManyReplyPaths);
        }
        // The authoritative store must carry at least one eligible
        // candidate; otherwise the lookup terminates honestly.
        let candidates =
            select_floodfill_candidates(&self.store, &target, routing_key, &[], &self.policy);
        if candidates.is_empty() {
            self.counters.lookups_failed = self.counters.lookups_failed.saturating_add(1);
            return Err(NetDbTunnelError::NoEligibleCandidates);
        }
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.checked_add(1).expect("request id");
        self.seam
            .set_reply_path_provider(Box::new(StaticReplyProvider { path: reply_path }));
        let action = self
            .seam
            .begin_lookup(&self.store, request_id, target, routing_key);
        match action {
            LookupAction::SendDatabaselookup { lookup_id, .. } => {
                let deadline_ms = self.now_ms.saturating_add(DEFAULT_LOOKUP_DEADLINE_MS);
                self.pending.insert(
                    request_id,
                    PendingLookup {
                        lookup_id,
                        target,
                        deadline_ms,
                        retries: 0,
                    },
                );
                self.retained_paths.insert(request_id, reply_path);
                self.counters.lookups_started = self.counters.lookups_started.saturating_add(1);
                Ok((request_id, action))
            }
            LookupAction::NeedExploratoryReplyPath { .. } => {
                self.counters.lookups_failed = self.counters.lookups_failed.saturating_add(1);
                Err(NetDbTunnelError::InvalidReplyPath)
            }
            LookupAction::Complete { .. } => {
                self.counters.lookups_failed = self.counters.lookups_failed.saturating_add(1);
                Err(NetDbTunnelError::NoEligibleCandidates)
            }
        }
    }

    /// Composes one outbound `DatabaseLookup` dispatch through the
    /// supplied outbound role. The helper is the only counted lookup
    /// path; direct SSU2 `DatabaseLookup` delivery is never a counted
    /// success (see [`Self::note_direct_transport_attempt`]).
    #[allow(clippy::too_many_arguments)]
    pub fn compose_lookup_via_tunnel<R: CryptoRng + RngCore>(
        &self,
        action: &LookupAction,
        role: &OutboundGatewayRole,
        message_id: u32,
        expiration_ms: u64,
        deadline: Deadline,
        rng: &mut R,
        now_ms: u64,
    ) -> Result<
        (
            crate::outbound_lookup::OutboundLookupDispatch,
            TunnelPathProof,
        ),
        NetDbTunnelError,
    > {
        let (peer, lookup_key) = match action {
            LookupAction::SendDatabaselookup { peer, message, .. } => {
                (*peer, Hash::from_bytes(*message.key.as_bytes()))
            }
            _ => return Err(NetDbTunnelError::Outbound("not a send action".to_owned())),
        };
        let dispatch = crate::outbound_lookup::compose_outbound_lookup(
            action,
            role,
            message_id,
            expiration_ms,
            deadline,
            rng,
            now_ms,
        )
        .map_err(|e| NetDbTunnelError::Outbound(e.to_string()))?;
        // Prove the dispatch traversed the tunnel path: every delivery
        // must decode as a complete short-transport TunnelData cell,
        // the delivery target must equal the role's first hop, and the
        // nested DatabaseLookup key must remain K.
        let first = dispatch
            .first()
            .ok_or_else(|| NetDbTunnelError::Outbound("no delivery in dispatch".to_owned()))?;
        let outer =
            I2npMessage::decode_short_transport(first.message_bytes(), MAX_I2NP_PAYLOAD_SIZE)
                .map_err(|e| NetDbTunnelError::Outbound(format!("{e:?}")))?;
        match outer.body() {
            I2npBody::TunnelData(_) => {}
            other => {
                return Err(NetDbTunnelError::Outbound(format!(
                    "outer is not TunnelData: {:?}",
                    other.message_type()
                )));
            }
        }
        let expected_first = role.established().first_hop_router().hash();
        if first.target() != i2pr_transport::PeerId::from_hash(expected_first) {
            return Err(NetDbTunnelError::Outbound(
                "delivery target is not the outbound first hop".to_owned(),
            ));
        }
        let proof = TunnelPathProof {
            cell_count: dispatch.cell_count,
            first_hop: expected_first,
            floodfill: Hash::from_bytes(*peer.as_bytes()),
            lookup_key,
            via_tunnel: true,
        };
        Ok((dispatch, proof))
    }

    /// Ingests one recovered `DatabaseStore` envelope into the lookup
    /// state machine through the ordinary store path.
    ///
    /// Mismatched key/store, stale/invalid, malformed, and duplicate
    /// responses are bounded and never complete the lookup.
    pub fn ingest_tunnel_store(
        &mut self,
        request_id: u64,
        envelope: &I2npMessage,
        context: ValidationContext,
    ) -> Result<ResponseOutcome, NetDbTunnelError> {
        let pending = self
            .pending
            .get(&request_id)
            .copied()
            .ok_or(NetDbTunnelError::UnknownLookup)?;
        if !matches!(envelope.body(), I2npBody::DatabaseStore(_)) {
            self.counters.malformed_rejected = self.counters.malformed_rejected.saturating_add(1);
            return Err(NetDbTunnelError::Malformed);
        }
        let outcome = crate::inbound_dispatch::route_databasestore(
            self.seam.lookup_mut(),
            &mut self.store,
            pending.lookup_id,
            envelope,
            context,
        )
        .map_err(|e| NetDbTunnelError::LookupEngine(e.to_string()))?;
        match &outcome {
            ResponseOutcome::Completed(result) => match **result {
                i2pr_netdb::LookupResult::Success { .. } => {
                    self.counters.lookups_succeeded =
                        self.counters.lookups_succeeded.saturating_add(1);
                    self.pending.remove(&request_id);
                    self.retained_paths.remove(&request_id);
                }
                _ => {
                    self.counters.mismatched_rejected =
                        self.counters.mismatched_rejected.saturating_add(1);
                }
            },
            ResponseOutcome::Continue => {
                // Mismatched key, stale/invalid signature, or
                // decompression failure all surface as Continue; the
                // lookup stays alive and bounded.
                self.counters.mismatched_rejected =
                    self.counters.mismatched_rejected.saturating_add(1);
            }
            ResponseOutcome::Ignored => {
                self.counters.duplicate_ignored = self.counters.duplicate_ignored.saturating_add(1);
            }
        }
        Ok(outcome)
    }

    /// Ingests one recovered `DatabaseSearchReply` envelope. Unknown
    /// peers never cause unbounded work; the policy caps suggestions.
    pub fn ingest_search_reply(
        &mut self,
        request_id: u64,
        envelope: &I2npMessage,
    ) -> Result<ResponseOutcome, NetDbTunnelError> {
        let pending = self
            .pending
            .get(&request_id)
            .copied()
            .ok_or(NetDbTunnelError::UnknownLookup)?;
        if !matches!(envelope.body(), I2npBody::DatabaseSearchReply(_)) {
            self.counters.malformed_rejected = self.counters.malformed_rejected.saturating_add(1);
            return Err(NetDbTunnelError::Malformed);
        }
        let before = self
            .seam
            .lookup()
            .diagnostics()
            .map(|d| d.suggestions_merged)
            .unwrap_or(0);
        let outcome = crate::inbound_dispatch::route_database_search_reply(
            self.seam.lookup_mut(),
            pending.lookup_id,
            envelope,
            &self.policy,
        )
        .map_err(|e| NetDbTunnelError::LookupEngine(e.to_string()))?;
        let after = self
            .seam
            .lookup()
            .diagnostics()
            .map(|d| d.suggestions_merged)
            .unwrap_or(before);
        self.counters.search_replies_merged = self
            .counters
            .search_replies_merged
            .saturating_add(after.saturating_sub(before) as u64);
        if matches!(outcome, ResponseOutcome::Ignored) {
            self.counters.duplicate_ignored = self.counters.duplicate_ignored.saturating_add(1);
        }
        Ok(outcome)
    }

    /// Forwards a delivery outcome for the pending lookup.
    pub fn lookup_delivery_outcome(
        &mut self,
        request_id: u64,
        outcome: i2pr_netdb::DeliveryOutcome,
    ) -> Result<ResponseOutcome, NetDbTunnelError> {
        let pending = self
            .pending
            .get(&request_id)
            .copied()
            .ok_or(NetDbTunnelError::UnknownLookup)?;
        let result =
            i2pr_netdb::handle_delivery_outcome(self.seam.lookup_mut(), pending.lookup_id, outcome)
                .map_err(|e| NetDbTunnelError::LookupEngine(e.to_string()))?;
        if matches!(result, ResponseOutcome::Ignored) {
            self.counters.duplicate_ignored = self.counters.duplicate_ignored.saturating_add(1);
        }
        Ok(result)
    }

    /// Cancels one pending lookup and frees its slot.
    pub fn cancel_lookup(&mut self, request_id: u64) -> Result<(), NetDbTunnelError> {
        if self.pending.remove(&request_id).is_none() {
            return Err(NetDbTunnelError::UnknownLookup);
        }
        self.retained_paths.remove(&request_id);
        self.seam.cancel();
        self.counters.cancellations = self.counters.cancellations.saturating_add(1);
        Ok(())
    }

    /// Expires pending lookups past their deadlines. Expired entries
    /// are removed and reported; they never spin.
    pub fn expire_lookups(&mut self) -> Vec<u64> {
        let now_ms = self.now_ms;
        let expired: Vec<u64> = self
            .pending
            .iter()
            .filter_map(|(id, pending)| {
                if pending.deadline_ms <= now_ms {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        for id in &expired {
            self.pending.remove(id);
            self.retained_paths.remove(id);
            self.counters.timeouts = self.counters.timeouts.saturating_add(1);
            self.counters.lookups_failed = self.counters.lookups_failed.saturating_add(1);
        }
        if !expired.is_empty() {
            self.seam.cancel();
        }
        expired
    }

    /// Reports tunnel loss during a lookup as a typed retry/failure.
    /// The caller may request replacement through Plan 185 policy;
    /// the coordinator never silently falls back to direct transport.
    pub fn note_tunnel_loss(&mut self, request_id: u64) -> Result<(), NetDbTunnelError> {
        let pending = self
            .pending
            .get_mut(&request_id)
            .ok_or(NetDbTunnelError::UnknownLookup)?;
        pending.retries = pending.retries.saturating_add(1);
        self.counters.tunnel_failures = self.counters.tunnel_failures.saturating_add(1);
        if pending.retries > MAX_LOOKUP_RETRIES {
            self.pending.remove(&request_id);
            self.retained_paths.remove(&request_id);
            self.seam.cancel();
            self.counters.lookups_failed = self.counters.lookups_failed.saturating_add(1);
            return Err(NetDbTunnelError::TunnelLost);
        }
        Ok(())
    }

    /// Rejects any direct-transport NetDB delivery as a counted path.
    /// The function always fails closed so evidence must prove TunnelData
    /// traversal.
    pub fn note_direct_transport_attempt(&mut self) -> Result<(), NetDbTunnelError> {
        self.counters.direct_rejections = self.counters.direct_rejections.saturating_add(1);
        Err(NetDbTunnelError::DirectTransportRejected)
    }

    /// Registers the locally signed RouterInfo snapshot the publication
    /// coordinator will publish. The coordinator never re-signs the
    /// record; retries reuse the supplied bytes.
    pub fn register_local(&mut self, local: i2pr_netdb::LocalRouterInfo) {
        self.publication.register_local(local);
    }

    /// Returns the closest eligible floodfill set for the local
    /// RouterHash through the existing selection logic.
    pub fn nearest_floodfills(&self) -> Vec<RouterHash> {
        self.publication.nearest_floodfills(&self.store)
    }

    /// Begins one publication attempt against the supplied floodfill
    /// peer. The peer must be an eligible floodfill in the
    /// authoritative store.
    pub fn begin_publication(
        &mut self,
        peer: RouterHash,
    ) -> Result<PublicationAttemptRecord, NetDbTunnelError> {
        if !self.is_floodfill(&peer) {
            return Err(NetDbTunnelError::NoEligibleCandidates);
        }
        let record = self
            .publication
            .begin_attempt(peer, &self.store)
            .map_err(|e| NetDbTunnelError::Publication(e.to_string()))?;
        self.counters.publications_started = self.counters.publications_started.saturating_add(1);
        Ok(record)
    }

    /// Composes one outbound RouterInfo publication through the
    /// supplied outbound role. Success requires protocol-derived
    /// acknowledgement/observation, never process exit alone.
    #[allow(clippy::too_many_arguments)]
    pub fn compose_publication_via_tunnel<R: CryptoRng + RngCore>(
        &self,
        record: &PublicationAttemptRecord,
        target_floodfill: Hash,
        role: &OutboundGatewayRole,
        message_id: u32,
        expiration_ms: u64,
        deadline: Deadline,
        rng: &mut R,
        now_ms: u64,
    ) -> Result<
        (
            crate::outbound_lookup::OutboundLookupDispatch,
            PublicationPathProof,
        ),
        NetDbTunnelError,
    > {
        let dispatch = crate::outbound_lookup::compose_outbound_publication(
            &record.store_message,
            target_floodfill,
            role,
            message_id,
            expiration_ms,
            deadline,
            rng,
            now_ms,
        )
        .map_err(|e| NetDbTunnelError::Outbound(e.to_string()))?;
        let first = dispatch
            .first()
            .ok_or_else(|| NetDbTunnelError::Outbound("no delivery in publication".to_owned()))?;
        let outer =
            I2npMessage::decode_short_transport(first.message_bytes(), MAX_I2NP_PAYLOAD_SIZE)
                .map_err(|e| NetDbTunnelError::Outbound(format!("{e:?}")))?;
        match outer.body() {
            I2npBody::TunnelData(_) => {}
            other => {
                return Err(NetDbTunnelError::Outbound(format!(
                    "publication outer is not TunnelData: {:?}",
                    other.message_type()
                )));
            }
        }
        let expected_first = role.established().first_hop_router().hash();
        if first.target() != i2pr_transport::PeerId::from_hash(expected_first) {
            return Err(NetDbTunnelError::Outbound(
                "publication delivery target is not the outbound first hop".to_owned(),
            ));
        }
        // The stored key must remain the local router hash, never the
        // floodfill; the tunnel ROUTER target carries the floodfill.
        if record.store_message.key == target_floodfill {
            return Err(NetDbTunnelError::Publication(
                "store key must be local, not floodfill".to_owned(),
            ));
        }
        let proof = PublicationPathProof {
            cell_count: dispatch.cell_count,
            first_hop: expected_first,
            floodfill: target_floodfill,
            via_tunnel: true,
        };
        Ok((dispatch, proof))
    }

    /// Marks a publication attempt as delivered (transport admission,
    /// not protocol acknowledgement).
    pub fn mark_publication_delivered(&mut self, request_id: u64) -> Result<(), NetDbTunnelError> {
        self.publication
            .mark_delivered(request_id)
            .map_err(|e| NetDbTunnelError::Publication(e.to_string()))
    }

    /// Marks a publication attempt as a delivery failure.
    pub fn mark_publication_failed(&mut self, request_id: u64) -> Result<(), NetDbTunnelError> {
        self.publication
            .mark_delivery_failed(request_id)
            .map_err(|e| NetDbTunnelError::Publication(e.to_string()))
    }

    /// Correlates a `DeliveryStatus` reply with a tracked publication
    /// attempt. A mismatched token is a typed rejection.
    pub fn correlate_publication_status(
        &mut self,
        message_id: u32,
    ) -> Result<i2pr_netdb::PublicationCorrelation, NetDbTunnelError> {
        let correlation = self
            .publication
            .correlate_delivery_status(message_id)
            .map_err(|e| NetDbTunnelError::Publication(e.to_string()))?;
        self.counters.publications_acked = self.counters.publications_acked.saturating_add(1);
        Ok(correlation)
    }

    /// Records a rejected `DeliveryStatus` for a publication attempt.
    pub fn record_publication_rejection(
        &mut self,
        request_id: u64,
    ) -> Result<(), NetDbTunnelError> {
        self.publication
            .record_rejection(request_id)
            .map_err(|e| NetDbTunnelError::Publication(e.to_string()))
    }

    /// Cancels a publication attempt.
    pub fn cancel_publication(&mut self, request_id: u64) -> Result<(), NetDbTunnelError> {
        self.publication
            .cancel(request_id)
            .map_err(|e| NetDbTunnelError::Publication(e.to_string()))
    }

    /// Retries a publication attempt without re-signing.
    pub fn retry_publication(&mut self, request_id: u64) -> Result<(), NetDbTunnelError> {
        self.publication
            .retry_attempt(request_id)
            .map_err(|e| NetDbTunnelError::Publication(e.to_string()))
    }

    /// Returns the bounded publication snapshot.
    pub fn publication_snapshot(&self) -> i2pr_netdb::PublicationSnapshot {
        self.publication.snapshot()
    }

    /// Returns the authoritative store stats.
    pub fn store_stats(&self) -> i2pr_netdb::RouterInfoStoreStats {
        self.store.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinator_starts_with_empty_authoritative_store() {
        let coord =
            NetDbTunnelCoordinator::new(LookupPolicy::default(), RouterInfoStoreConfig::default());
        assert_eq!(coord.store().len(), 0);
        assert_eq!(coord.floodfill_count(), 0);
        assert_eq!(coord.pending_len(), 0);
    }

    #[test]
    fn direct_transport_is_never_counted() {
        let mut coord =
            NetDbTunnelCoordinator::new(LookupPolicy::default(), RouterInfoStoreConfig::default());
        let error = coord
            .note_direct_transport_attempt()
            .expect_err("direct must reject");
        assert_eq!(error, NetDbTunnelError::DirectTransportRejected);
        assert_eq!(coord.counters().direct_rejections, 1);
    }
}
