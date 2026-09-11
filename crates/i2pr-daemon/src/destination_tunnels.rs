//! Plan 187 daemon-owned destination LeaseSet2 and Garlic routing
//! coordinator (extended by Plan 190 with the typed inbound-gateway
//! route adapter).
//!
//! The coordinator connects the existing destination message plane to
//! the live Plan 185/186 tunnel and NetDB substrate:
//!
//! ```text
//! application/destination asks for remote peer
//!  -> bounded LeaseSet2 lookup through the authoritative store
//!  -> real exploratory outbound tunnel (existing composition)
//!  -> reference floodfill
//!  -> reply via real exploratory inbound tunnel
//!     (Plan 190: reply path derives from typed InboundGatewayRoute
//!      so the encoded DatabaseLookup advertises the remote gateway
//!      receive id, not the local endpoint id)
//!  -> signed Standard LeaseSet2 validation (existing validators)
//!  -> LeaseSet2Store/cache
//!  -> destination routing retry over real destination tunnels
//! ```
//!
//! Properties:
//!
//! - one bounded authoritative [`i2pr_netdb::RouterInfoStore`]
//!   populated only by validated signed RouterInfo records; the live
//!   LeaseSet2 lookup path never consults the Plan 122 placeholder
//!   empty store (Plan 187 §5 drives
//!   [`crate::netdb_seam::NetDbSeam::begin_lease_set2_lookup_with_store`]);
//! - bootstrap traverses the same parser/signature/freshness
//!   validation used for ordinary records before eligibility;
//! - floodfill selection reuses the existing
//!   [`i2pr_netdb::select_floodfill_candidates`] routing-key logic;
//! - outbound dispatch reuses
//!   [`crate::outbound_lookup::compose_outbound_lookup`] /
//!   [`crate::outbound_lookup::compose_outbound_publication`] through
//!   the supplied [`i2pr_tunnel::roles::OutboundGatewayRole`]; direct
//!   destination-over-SSU2 delivery is never a counted path;
//! - inbound recovery reuses
//!   [`crate::inbound_dispatch::dispatch_inbound_tunnel_data`]; NetDB
//!   envelopes ingest through the [`crate::netdb_seam::NetDbSeam`]
//!   LeaseSet2 surface into the authoritative
//!   [`i2pr_netdb::LeaseSet2Store`], while Garlic envelopes recover
//!   as opaque bytes for the existing client-layer ECIES dispatch;
//! - local LeaseSet2 publication reuses the existing outbound
//!   publication composition with protocol-derived `DeliveryStatus`
//!   acknowledgement; retries reuse the supplied signed bytes and
//!   never re-sign;
//! - destination tunnel material is verified remote: counted rows
//!   reject `LocalZeroHop` registrations and require usable real
//!   inbound lease sources plus a real outbound route;
//! - the `reply_path_for_inbound_route` adapter (Plan 190) is the only
//!   place production code constructs a tunneled-reply `ReplyPath`:
//!   it derives the path from the typed
//!   [`i2pr_tunnel::data_plane_registry::InboundGatewayRoute`] so the
//!   local creator endpoint receive tunnel id is impossible to copy
//!   into `DatabaseLookup.reply_tunnelId`. Missing / zero / local-id-as-
//!   reply-id all fail closed with typed `ReplyPathDerivationError`.
//! - concurrent lookups/publications, retained reply paths, store
//!   entries/bytes, and retries/deadlines are all bounded;
//! - tunnel loss returns a typed retry/failure and never silently
//!   falls back to direct transport.
//!
//! The module is runtime-neutral: it never opens sockets, never spawns
//! tasks, never performs DNS, never touches the filesystem. All I/O
//! happens through the caller-supplied tunnel roles and registry. It
//! never logs destination key material, session secrets, or
//! application payload bytes; evidence surfaces carry only digests,
//! counts, and hashes.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use i2pr_client::DestinationTunnelPool;
use i2pr_netdb::{
    DestinationHash, LeaseSet2Store, LookupAction, LookupId, LookupPolicy, ReplyPath,
    ReplyPathError, ResponseOutcome, RouterHash, RouterInfoStore, RouterInfoStoreConfig,
    router_hash_from_destination, select_floodfill_candidates,
};
use i2pr_proto::{Hash, I2npBody, I2npMessage, MAX_COMMON_STRUCTURE_SIZE, MAX_I2NP_PAYLOAD_SIZE};
use i2pr_transport::Deadline;
use i2pr_tunnel::data_plane_registry::{DataPlaneRegistry, InboundGatewayRoute};
use i2pr_tunnel::roles::OutboundGatewayRole;
use rand_core::{CryptoRng, RngCore};
use thiserror::Error;

/// Maximum concurrent remote LeaseSet2 lookups the coordinator tracks.
pub const MAX_CONCURRENT_LEASE_LOOKUPS: usize = 8;
/// Maximum retained inbound reply paths.
pub const MAX_RETAINED_LEASE_REPLY_PATHS: usize = 8;
/// Maximum concurrent local LeaseSet2 publications.
pub const MAX_CONCURRENT_LEASE_PUBLICATIONS: usize = 8;
/// Maximum lookup retries before the coordinator reports peer-exhausted.
pub const MAX_LEASE_LOOKUP_RETRIES: u32 = 3;
/// Default lookup deadline in milliseconds (matches the lookup policy total).
pub const DEFAULT_LEASE_LOOKUP_DEADLINE_MS: u64 = 5_000;
/// Starting request identifier.
pub const INITIAL_DESTINATION_REQUEST_ID: u64 = 1;

/// Typed failures for the destination-over-tunnels coordinator.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DestinationTunnelError {
    /// Too many concurrent LeaseSet2 lookups are active.
    #[error("too many concurrent destination lookups")]
    TooManyLookups,
    /// Too many retained reply paths are held.
    #[error("too many retained destination reply paths")]
    TooManyReplyPaths,
    /// Too many concurrent LeaseSet2 publications are active.
    #[error("too many concurrent destination publications")]
    TooManyPublications,
    /// No eligible floodfill candidate exists in the authoritative store.
    #[error("no eligible floodfill candidate")]
    NoEligibleCandidates,
    /// The lookup identifier is unknown.
    #[error("unknown destination lookup")]
    UnknownLookup,
    /// The publication identifier is unknown.
    #[error("unknown destination publication")]
    UnknownPublication,
    /// The supplied reply path is invalid.
    #[error("invalid destination reply path")]
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
    /// Direct-transport destination delivery is never a counted path.
    #[error("direct-transport destination delivery is not a counted path")]
    DirectTransportRejected,
    /// The tunnel was lost during the operation.
    #[error("tunnel lost during destination operation")]
    TunnelLost,
    /// The lookup deadline elapsed.
    #[error("destination lookup deadline elapsed")]
    DeadlineExceeded,
    /// The recovered envelope was malformed.
    #[error("recovered destination envelope is malformed")]
    Malformed,
    /// The lookup was cancelled.
    #[error("destination lookup was cancelled")]
    Cancelled,
    /// A counted remote row attempted to use local zero-hop tunnel
    /// material (Plan 187 §4 forbids `LocalZeroHop` for counted i2pr
    /// remote evidence).
    #[error("counted remote row requires real tunnel material, not local zero-hop")]
    LocalZeroHopForCountedPath,
    /// The destination pool carries no usable real inbound lease.
    #[error("destination pool has no usable real inbound lease")]
    NoInboundLease,
    /// The destination pool carries no real outbound route.
    #[error("destination pool has no real outbound route")]
    NoOutboundRoute,
    /// The destination pool is not usable at the supplied time.
    #[error("destination pool is not usable")]
    PoolNotUsable,
    /// The inbound cell did not complete a message.
    #[error("inbound cell did not complete a message")]
    CellIncomplete,
    /// The recovered envelope is not a destination Garlic message.
    #[error("recovered envelope is not a destination Garlic message (type 11={type_byte})")]
    UnexpectedBodyType {
        /// I2NP type byte of the rejected envelope.
        type_byte: u8,
    },
    /// The inbound tunnel id does not match an activated endpoint.
    #[error("unknown inbound tunnel id {0}")]
    UnknownInboundTunnel(u32),
}

/// Privacy-safe bounded counters for the coordinator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DestinationTunnelCounters {
    /// LeaseSet2 lookups started through the authoritative store.
    pub lookups_started: u64,
    /// Lookups that reached terminal validation + install.
    pub lookups_succeeded: u64,
    /// Lookups that terminated without success.
    pub lookups_failed: u64,
    /// Local LeaseSet2 publications started.
    pub publications_started: u64,
    /// Publications acknowledged via protocol-derived evidence.
    pub publications_acked: u64,
    /// Search-reply responses processed (bounded).
    pub search_replies_merged: u64,
    /// Mismatched key/store responses rejected.
    pub mismatched_rejected: u64,
    /// Stale/invalid LeaseSet2 responses rejected.
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
    /// Garlic envelopes recovered through real inbound tunnels.
    pub garlic_recovered: u64,
    /// Remote material proofs issued.
    pub material_proofs: u64,
    /// Pending lookups at the snapshot.
    pub pending_lookups: usize,
    /// Retained reply paths at the snapshot.
    pub retained_paths: usize,
    /// Pending publications at the snapshot.
    pub pending_publications: usize,
}

/// Proof that one destination lookup traversed the tunnel path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestinationTunnelPathProof {
    /// Number of TunnelData cells emitted.
    pub cell_count: usize,
    /// Outbound first-hop router (delivery target P).
    pub first_hop: Hash,
    /// Selected floodfill peer (tunnel ROUTER target F).
    pub floodfill: Hash,
    /// DatabaseLookup key (K, the destination-derived router hash).
    pub lookup_key: Hash,
    /// Always true; the type exists so callers cannot substitute a
    /// direct-transport send for a counted row.
    pub via_tunnel: bool,
}

impl std::fmt::Display for DestinationTunnelPathProof {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "destination-tunnel-path(cells={}, via_tunnel={})",
            self.cell_count, self.via_tunnel
        )
    }
}

/// Proof that one destination owns real non-zero-hop tunnel material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteMaterialProof {
    /// Usable real inbound lease sources at proof time.
    pub inbound_leases: usize,
    /// Registered real outbound routes at proof time.
    pub outbound_routes: usize,
    /// Always false on success; the type records that zero-hop
    /// material was checked and rejected for the counted row.
    pub zero_hop: bool,
}

/// Privacy-safe summary of one cached remote Standard LeaseSet2.
/// Carries counts and validity bounds only; never key material or
/// payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteLeaseSummary {
    /// Resolved destination hash.
    pub destination: DestinationHash,
    /// Lease count in the validated record.
    pub lease_count: usize,
    /// Publication timestamp (seconds).
    pub published_seconds: u32,
    /// Expiry timestamp (seconds).
    pub expires_seconds: u32,
}

/// Outcome of ingesting one tunnel-recovered LeaseSet2 store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeaseStoreIngestOutcome {
    /// The lookup completed with a validated cached record.
    Completed {
        /// Completed lookup identity.
        lookup_id: LookupId,
        /// Privacy-safe record summary.
        summary: RemoteLeaseSummary,
    },
    /// The state machine kept the lookup alive.
    Continue,
    /// The state machine was already terminal; the response was ignored.
    Ignored,
}

/// One pending LeaseSet2 lookup tracked by the coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingLeaseLookup {
    lookup_id: LookupId,
    target: DestinationHash,
    deadline_ms: u64,
    retries: u32,
}

/// One pending local LeaseSet2 publication tracked by the coordinator.
#[derive(Clone, Debug)]
struct PendingLeasePublication {
    floodfill: RouterHash,
    store_message: i2pr_proto::DatabaseStoreMessage,
    message_id: u32,
}

/// Test/adapters reply-path provider. Production callers inject the
/// real inbound tunnel path per lookup; the provider never consults
/// transport state.
#[derive(Debug)]
struct LeaseReplyProvider {
    path: ReplyPath,
}

impl i2pr_netdb::ReplyPathProvider for LeaseReplyProvider {
    fn has_inbound_tunnel(&self) -> bool {
        true
    }

    fn provide_reply_path(&self) -> Option<ReplyPath> {
        Some(self.path)
    }
}

/// Failures for [`reply_path_for_inbound_route`] (Plan 190 §5.2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyPathDerivationError {
    /// The supplied local receive tunnel id does not map to an
    /// activated inbound exploratory role. Missing metadata is
    /// fail-closed: the caller must re-register the tunnel rather
    /// than synthesize a path.
    MissingRoute,
    /// The derived path's tunnel id was zero. The registry never
    /// retains zero tunnel ids, but the failure is preserved as a
    /// typed boundary so test/lint harnesses can prove it never
    /// happens.
    ZeroTunnelId,
    /// The derived path's tunnel id was *equal* to the local
    /// receive tunnel id. That is the exact defect Plan 190 fixes
    /// and is preserved as a typed boundary so callers cannot
    /// silently regress.
    LocalIdUsedAsReplyTunnel,
}

impl std::fmt::Display for ReplyPathDerivationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRoute => formatter.write_str(
                "no inbound gateway route registered for the local receive tunnel id",
            ),
            Self::ZeroTunnelId => formatter.write_str("reply path tunnel id must be nonzero"),
            Self::LocalIdUsedAsReplyTunnel => formatter.write_str(
                "reply path tunnel id must be the remote gateway receive id, not the local endpoint id",
            ),
        }
    }
}

impl std::error::Error for ReplyPathDerivationError {}

/// Plan 190 §5.2 daemon-owned adapter that converts one registered
/// inbound exploratory route into an `i2pr_netdb::ReplyPath`.
///
/// The adapter is the **only** place production code constructs a
/// tunneled-reply `ReplyPath`. It deliberately uses
/// `(gateway_router, gateway_receive_tunnel)` and **never** the
/// `local_receive_tunnel`, because the I2NP contract for a tunneled
/// `DatabaseLookup` requires the receive id on the remote inbound
/// gateway.
///
/// Missing / zero / local-id-as-reply-id all fail closed; there is
/// no direct-transport fallback, no `LocalZeroHop` path, and no
/// silent substitution. Test harnesses that previously built
/// `ReplyPath` from the local receive id should call this helper
/// instead.
pub fn reply_path_for_inbound_route(
    registry: &DataPlaneRegistry,
    local_receive: i2pr_tunnel::identity::TunnelId,
) -> Result<ReplyPath, ReplyPathDerivationError> {
    let route = registry
        .inbound_gateway_route(local_receive)
        .ok_or(ReplyPathDerivationError::MissingRoute)?;
    let path = reply_path_for_route(route)?;
    if path.tunnel_id() == local_receive.get() {
        return Err(ReplyPathDerivationError::LocalIdUsedAsReplyTunnel);
    }
    Ok(path)
}

/// Internal mapping from the typed [`InboundGatewayRoute`] to an
/// `i2pr_netdb::ReplyPath`. The function is small and private so
/// callers cannot bypass the typed route input.
fn reply_path_for_route(route: InboundGatewayRoute) -> Result<ReplyPath, ReplyPathDerivationError> {
    if route.gateway_receive_tunnel.get() == 0 {
        return Err(ReplyPathDerivationError::ZeroTunnelId);
    }
    ReplyPath::new(
        RouterHash::from_bytes(*route.gateway_router.as_bytes()),
        route.gateway_receive_tunnel.get(),
    )
    .map_err(|error: ReplyPathError| match error {
        ReplyPathError::ZeroTunnelId => ReplyPathDerivationError::ZeroTunnelId,
    })
}

/// Daemon-owned destination LeaseSet2/Garlic-over-tunnels coordinator.
pub struct DestinationTunnelCoordinator {
    store: RouterInfoStore,
    policy: LookupPolicy,
    seam: crate::netdb_seam::NetDbSeam,
    lease_store: LeaseSet2Store,
    pending: BTreeMap<u64, PendingLeaseLookup>,
    retained_paths: BTreeMap<u64, ReplyPath>,
    publications: BTreeMap<u64, PendingLeasePublication>,
    next_request_id: u64,
    now_ms: u64,
    counters: DestinationTunnelCounters,
}

impl DestinationTunnelCoordinator {
    /// Constructs a coordinator with the supplied bounded policy and
    /// authoritative store configuration.
    pub fn new(policy: LookupPolicy, store_config: RouterInfoStoreConfig) -> Self {
        Self {
            store: RouterInfoStore::with_config(store_config),
            policy,
            seam: crate::netdb_seam::NetDbSeam::new(policy),
            lease_store: LeaseSet2Store::default(),
            pending: BTreeMap::new(),
            retained_paths: BTreeMap::new(),
            publications: BTreeMap::new(),
            next_request_id: INITIAL_DESTINATION_REQUEST_ID,
            now_ms: 0,
            counters: DestinationTunnelCounters::default(),
        }
    }

    /// Returns the authoritative bounded RouterInfo store.
    pub fn store(&self) -> &RouterInfoStore {
        &self.store
    }

    /// Returns the validated remote LeaseSet2 cache.
    pub fn lease_store(&self) -> &LeaseSet2Store {
        &self.lease_store
    }

    /// Returns the bounded lookup policy.
    pub const fn policy(&self) -> LookupPolicy {
        self.policy
    }

    /// Returns the bounded counters snapshot.
    pub fn counters(&self) -> DestinationTunnelCounters {
        let mut snapshot = self.counters;
        snapshot.pending_lookups = self.pending.len();
        snapshot.retained_paths = self.retained_paths.len();
        snapshot.pending_publications = self.publications.len();
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

    /// Returns the number of pending publications.
    pub fn publications_len(&self) -> usize {
        self.publications.len()
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
        now: i2pr_proto::Date,
    ) -> Result<RouterHash, DestinationTunnelError> {
        let info = i2pr_proto::RouterInfo::decode(bytes, MAX_COMMON_STRUCTURE_SIZE)
            .map_err(|e| DestinationTunnelError::ReferenceRejected(format!("{e:?}")))?;
        let context = i2pr_netdb::ValidationContext::new(now);
        let validated = i2pr_netdb::ValidatedRouterInfo::from_router_info(info, None, context)
            .map_err(|e| DestinationTunnelError::ReferenceRejected(e.to_string()))?;
        let key = validated.key();
        let outcome = self.store.insert(validated);
        match outcome {
            i2pr_netdb::InsertOutcome::Inserted
            | i2pr_netdb::InsertOutcome::Replaced
            | i2pr_netdb::InsertOutcome::Idempotent => Ok(key),
            i2pr_netdb::InsertOutcome::Conflict => Err(DestinationTunnelError::ReferenceRejected(
                "conflict".to_owned(),
            )),
            i2pr_netdb::InsertOutcome::StaleReplacement => Err(
                DestinationTunnelError::ReferenceRejected("stale".to_owned()),
            ),
            i2pr_netdb::InsertOutcome::CapacityExceeded => Err(
                DestinationTunnelError::ReferenceRejected("capacity".to_owned()),
            ),
        }
    }

    /// Returns whether the supplied hash is a floodfill advertiser in
    /// the authoritative store.
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
    /// destination/routing key through the authoritative store. The
    /// live path never consults the placeholder empty store.
    pub fn candidate_hashes(
        &self,
        target: &DestinationHash,
        routing_key: &RouterHash,
    ) -> Vec<RouterHash> {
        let target_hash = router_hash_from_destination(*target);
        select_floodfill_candidates(&self.store, &target_hash, routing_key, &[], &self.policy)
            .hashes()
    }

    /// Returns a privacy-safe summary of the cached remote LeaseSet2,
    /// if present.
    pub fn remote_lease_summary(
        &self,
        destination: &DestinationHash,
    ) -> Option<RemoteLeaseSummary> {
        let validated = self.lease_store.get(destination)?;
        let lease_set2 = validated.lease_set2();
        Some(RemoteLeaseSummary {
            destination: *destination,
            lease_count: lease_set2.leases().len(),
            published_seconds: lease_set2.published_seconds(),
            expires_seconds: lease_set2.expires_seconds(),
        })
    }

    /// Verifies the destination pool owns real non-zero-hop tunnel
    /// material for a counted remote row (Plan 187 §4).
    ///
    /// Any registered `LocalZeroHop` entry rejects the proof: counted
    /// i2pr remote evidence must traverse real established material.
    /// Success requires at least one usable real inbound lease source
    /// and at least one real outbound route, plus pool usability at
    /// the supplied time.
    pub fn verify_remote_material(
        &mut self,
        pool: &DestinationTunnelPool,
        now_seconds: u64,
    ) -> Result<RemoteMaterialProof, DestinationTunnelError> {
        if pool.zero_hop_inbound().is_some() || pool.zero_hop_outbound().is_some() {
            return Err(DestinationTunnelError::LocalZeroHopForCountedPath);
        }
        let inbound_leases = pool.inbound_lease_sources(now_seconds).len();
        if inbound_leases == 0 {
            return Err(DestinationTunnelError::NoInboundLease);
        }
        let outbound_routes = pool.outbound_len();
        if outbound_routes == 0 {
            return Err(DestinationTunnelError::NoOutboundRoute);
        }
        if !pool.is_usable(now_seconds) {
            return Err(DestinationTunnelError::PoolNotUsable);
        }
        self.counters.material_proofs = self.counters.material_proofs.saturating_add(1);
        Ok(RemoteMaterialProof {
            inbound_leases,
            outbound_routes,
            zero_hop: false,
        })
    }

    /// Begins one remote Standard LeaseSet2 lookup through the
    /// authoritative bounded store for real candidate selection.
    ///
    /// The caller supplies the real inbound exploratory reply path
    /// (selected from the Plan 185 registry pair). The returned
    /// action must be [`LookupAction::SendDatabaselookup`] for a
    /// counted row. Terminal `NoEligibleCandidates` outcomes
    /// terminate honestly rather than spinning.
    pub fn begin_lease_lookup(
        &mut self,
        target: DestinationHash,
        routing_key: &RouterHash,
        reply_path: ReplyPath,
    ) -> Result<(u64, LookupAction), DestinationTunnelError> {
        if self.pending.len() >= MAX_CONCURRENT_LEASE_LOOKUPS {
            return Err(DestinationTunnelError::TooManyLookups);
        }
        if self.retained_paths.len() >= MAX_RETAINED_LEASE_REPLY_PATHS
            && !self.retained_paths.values().any(|p| *p == reply_path)
        {
            return Err(DestinationTunnelError::TooManyReplyPaths);
        }
        // The authoritative store must carry at least one eligible
        // candidate; otherwise the lookup terminates honestly.
        let target_hash = router_hash_from_destination(target);
        let candidates =
            select_floodfill_candidates(&self.store, &target_hash, routing_key, &[], &self.policy);
        if candidates.is_empty() {
            self.counters.lookups_failed = self.counters.lookups_failed.saturating_add(1);
            return Err(DestinationTunnelError::NoEligibleCandidates);
        }
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.checked_add(1).expect("request id");
        self.seam
            .set_lease_set2_reply_path_provider(Box::new(LeaseReplyProvider { path: reply_path }));
        let action = self.seam.begin_lease_set2_lookup_with_store(
            &self.store,
            request_id,
            target,
            routing_key,
        );
        match action {
            LookupAction::SendDatabaselookup { lookup_id, .. } => {
                let deadline_ms = self.now_ms.saturating_add(DEFAULT_LEASE_LOOKUP_DEADLINE_MS);
                self.pending.insert(
                    request_id,
                    PendingLeaseLookup {
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
                Err(DestinationTunnelError::InvalidReplyPath)
            }
            LookupAction::Complete { .. } => {
                self.counters.lookups_failed = self.counters.lookups_failed.saturating_add(1);
                Err(DestinationTunnelError::NoEligibleCandidates)
            }
        }
    }

    /// Composes one outbound LeaseSet2 `DatabaseLookup` dispatch
    /// through the supplied outbound role. The helper is the only
    /// counted lookup path; direct transport delivery is never a
    /// counted success (see [`Self::note_direct_transport_attempt`]).
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
            DestinationTunnelPathProof,
        ),
        DestinationTunnelError,
    > {
        let (peer, lookup_key) = match action {
            LookupAction::SendDatabaselookup { peer, message, .. } => {
                (*peer, Hash::from_bytes(*message.key.as_bytes()))
            }
            _ => {
                return Err(DestinationTunnelError::Outbound(
                    "not a send action".to_owned(),
                ));
            }
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
        .map_err(|e| DestinationTunnelError::Outbound(e.to_string()))?;
        // Prove the dispatch traversed the tunnel path: every delivery
        // must decode as a complete short-transport TunnelData cell,
        // the delivery target must equal the role's first hop, and the
        // nested DatabaseLookup key must remain K.
        let first = dispatch.first().ok_or_else(|| {
            DestinationTunnelError::Outbound("no delivery in dispatch".to_owned())
        })?;
        let outer =
            I2npMessage::decode_short_transport(first.message_bytes(), MAX_I2NP_PAYLOAD_SIZE)
                .map_err(|e| DestinationTunnelError::Outbound(format!("{e:?}")))?;
        match outer.body() {
            I2npBody::TunnelData(_) => {}
            other => {
                return Err(DestinationTunnelError::Outbound(format!(
                    "outer is not TunnelData: {:?}",
                    other.message_type()
                )));
            }
        }
        let expected_first = role.established().first_hop_router().hash();
        if first.target() != i2pr_transport::PeerId::from_hash(expected_first) {
            return Err(DestinationTunnelError::Outbound(
                "delivery target is not the outbound first hop".to_owned(),
            ));
        }
        let proof = DestinationTunnelPathProof {
            cell_count: dispatch.cell_count,
            first_hop: expected_first,
            floodfill: Hash::from_bytes(*peer.as_bytes()),
            lookup_key,
            via_tunnel: true,
        };
        Ok((dispatch, proof))
    }

    /// Ingests one tunnel-recovered `DatabaseStore` envelope into the
    /// LeaseSet2 lookup state machine through the ordinary
    /// validation path into the authoritative LeaseSet2 cache.
    ///
    /// Destination binding, signature, expiry, lease presence,
    /// supported encryption type, and cache freshness all traverse
    /// the existing validators. Mismatched key/store, stale/invalid,
    /// malformed, and duplicate responses are bounded and never
    /// complete the lookup.
    pub fn ingest_tunnel_lease_store(
        &mut self,
        request_id: u64,
        envelope: &I2npMessage,
        now_seconds: u32,
    ) -> Result<LeaseStoreIngestOutcome, DestinationTunnelError> {
        let pending = self
            .pending
            .get(&request_id)
            .copied()
            .ok_or(DestinationTunnelError::UnknownLookup)?;
        if !matches!(envelope.body(), I2npBody::DatabaseStore(_)) {
            self.counters.malformed_rejected = self.counters.malformed_rejected.saturating_add(1);
            return Err(DestinationTunnelError::Malformed);
        }
        self.seam.set_lease_set2_now_seconds(now_seconds);
        let outcome = self
            .seam
            .ingest_lease_set2_response(&mut self.lease_store, envelope)
            .map_err(|e| DestinationTunnelError::LookupEngine(e.to_string()))?;
        match outcome {
            crate::netdb_seam::LeaseSet2ResponseOutcome::Completed(result) => match *result {
                i2pr_netdb::LookupResult::LeaseSet2Success {
                    lookup_id,
                    lease_set2,
                } => {
                    let validated = lease_set2.as_ref();
                    if validated.key() != pending.target {
                        self.counters.mismatched_rejected =
                            self.counters.mismatched_rejected.saturating_add(1);
                        return Err(DestinationTunnelError::LookupEngine(
                            "destination binding mismatch".to_owned(),
                        ));
                    }
                    let summary = RemoteLeaseSummary {
                        destination: validated.key(),
                        lease_count: validated.lease_set2().leases().len(),
                        published_seconds: validated.lease_set2().published_seconds(),
                        expires_seconds: validated.lease_set2().expires_seconds(),
                    };
                    self.counters.lookups_succeeded =
                        self.counters.lookups_succeeded.saturating_add(1);
                    self.pending.remove(&request_id);
                    self.retained_paths.remove(&request_id);
                    Ok(LeaseStoreIngestOutcome::Completed { lookup_id, summary })
                }
                _ => {
                    self.counters.mismatched_rejected =
                        self.counters.mismatched_rejected.saturating_add(1);
                    Err(DestinationTunnelError::LookupEngine(
                        "unexpected lookup result variant".to_owned(),
                    ))
                }
            },
            crate::netdb_seam::LeaseSet2ResponseOutcome::Continue => {
                // Mismatched key, stale/invalid signature, or
                // unsupported-encryption responses all surface as
                // Continue; the lookup stays alive and bounded.
                self.counters.mismatched_rejected =
                    self.counters.mismatched_rejected.saturating_add(1);
                Ok(LeaseStoreIngestOutcome::Continue)
            }
            crate::netdb_seam::LeaseSet2ResponseOutcome::Ignored => {
                self.counters.duplicate_ignored = self.counters.duplicate_ignored.saturating_add(1);
                Ok(LeaseStoreIngestOutcome::Ignored)
            }
        }
    }

    /// Ingests one recovered `DatabaseSearchReply` envelope. Unknown
    /// peers never cause unbounded work; the policy caps suggestions.
    pub fn ingest_search_reply(
        &mut self,
        request_id: u64,
        envelope: &I2npMessage,
    ) -> Result<ResponseOutcome, DestinationTunnelError> {
        let _pending = self
            .pending
            .get(&request_id)
            .copied()
            .ok_or(DestinationTunnelError::UnknownLookup)?;
        if !matches!(envelope.body(), I2npBody::DatabaseSearchReply(_)) {
            self.counters.malformed_rejected = self.counters.malformed_rejected.saturating_add(1);
            return Err(DestinationTunnelError::Malformed);
        }
        let outcome = self
            .seam
            .ingest_lease_set2_search_reply(envelope, &self.policy)
            .map_err(|e| DestinationTunnelError::LookupEngine(e.to_string()))?;
        self.counters.search_replies_merged = self.counters.search_replies_merged.saturating_add(1);
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
    ) -> Result<crate::netdb_seam::LeaseSet2ResponseOutcome, DestinationTunnelError> {
        if !self.pending.contains_key(&request_id) {
            return Err(DestinationTunnelError::UnknownLookup);
        }
        let result = self
            .seam
            .lease_set2_delivery_outcome(outcome)
            .map_err(|e| DestinationTunnelError::LookupEngine(e.to_string()))?;
        if matches!(result, crate::netdb_seam::LeaseSet2ResponseOutcome::Ignored) {
            self.counters.duplicate_ignored = self.counters.duplicate_ignored.saturating_add(1);
        }
        Ok(result)
    }

    /// Cancels one pending lookup and frees its slot.
    pub fn cancel_lookup(&mut self, request_id: u64) -> Result<(), DestinationTunnelError> {
        if self.pending.remove(&request_id).is_none() {
            return Err(DestinationTunnelError::UnknownLookup);
        }
        self.retained_paths.remove(&request_id);
        self.seam.cancel_lease_set2_lookup();
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
            self.seam.cancel_lease_set2_lookup();
        }
        expired
    }

    /// Reports tunnel loss during a lookup as a typed retry/failure.
    /// The caller may request replacement through Plan 185 policy;
    /// the coordinator never silently falls back to direct transport.
    pub fn note_tunnel_loss(&mut self, request_id: u64) -> Result<(), DestinationTunnelError> {
        let pending = self
            .pending
            .get_mut(&request_id)
            .ok_or(DestinationTunnelError::UnknownLookup)?;
        pending.retries = pending.retries.saturating_add(1);
        self.counters.tunnel_failures = self.counters.tunnel_failures.saturating_add(1);
        if pending.retries > MAX_LEASE_LOOKUP_RETRIES {
            self.pending.remove(&request_id);
            self.retained_paths.remove(&request_id);
            self.seam.cancel_lease_set2_lookup();
            self.counters.lookups_failed = self.counters.lookups_failed.saturating_add(1);
            return Err(DestinationTunnelError::TunnelLost);
        }
        Ok(())
    }

    /// Rejects any direct-transport destination delivery as a counted
    /// path. The function always fails closed so evidence must prove
    /// TunnelData traversal.
    pub fn note_direct_transport_attempt(&mut self) -> Result<(), DestinationTunnelError> {
        self.counters.direct_rejections = self.counters.direct_rejections.saturating_add(1);
        Err(DestinationTunnelError::DirectTransportRejected)
    }

    /// Begins one local Standard LeaseSet2 publication attempt
    /// against an eligible floodfill peer (Plan 187 §8).
    ///
    /// The supplied store message must carry the destination's own
    /// signed record describing its real inbound tunnel; the stored
    /// key must remain destination-derived, never the floodfill.
    pub fn begin_ls2_publication(
        &mut self,
        store_message: i2pr_proto::DatabaseStoreMessage,
        floodfill: RouterHash,
    ) -> Result<u64, DestinationTunnelError> {
        if self.publications.len() >= MAX_CONCURRENT_LEASE_PUBLICATIONS {
            return Err(DestinationTunnelError::TooManyPublications);
        }
        if !self.is_floodfill(&floodfill) {
            return Err(DestinationTunnelError::NoEligibleCandidates);
        }
        if store_message.key == Hash::from_bytes(*floodfill.as_bytes()) {
            return Err(DestinationTunnelError::Publication(
                "store key must be destination-derived, not floodfill".to_owned(),
            ));
        }
        if !matches!(
            store_message.data,
            i2pr_proto::DatabaseStoreData::LeaseSet2(_)
        ) {
            return Err(DestinationTunnelError::Publication(
                "publication body must be a Standard LeaseSet2".to_owned(),
            ));
        }
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.checked_add(1).expect("request id");
        self.publications.insert(
            request_id,
            PendingLeasePublication {
                floodfill,
                store_message,
                message_id: 0,
            },
        );
        self.counters.publications_started = self.counters.publications_started.saturating_add(1);
        Ok(request_id)
    }

    /// Composes one outbound local LeaseSet2 publication through the
    /// supplied outbound role. Success requires protocol-derived
    /// acknowledgement, never transport admission alone.
    #[allow(clippy::too_many_arguments)]
    pub fn compose_ls2_publication_via_tunnel<R: CryptoRng + RngCore>(
        &mut self,
        request_id: u64,
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
            crate::netdb_tunnels::PublicationPathProof,
        ),
        DestinationTunnelError,
    > {
        let pending = self
            .publications
            .get(&request_id)
            .ok_or(DestinationTunnelError::UnknownPublication)?;
        if Hash::from_bytes(*pending.floodfill.as_bytes()) != target_floodfill {
            return Err(DestinationTunnelError::Publication(
                "publication floodfill mismatch".to_owned(),
            ));
        }
        let dispatch = crate::outbound_lookup::compose_outbound_publication(
            &pending.store_message,
            target_floodfill,
            role,
            message_id,
            expiration_ms,
            deadline,
            rng,
            now_ms,
        )
        .map_err(|e| DestinationTunnelError::Outbound(e.to_string()))?;
        let first = dispatch
            .first()
            .ok_or_else(|| DestinationTunnelError::Outbound("no delivery".to_owned()))?;
        let outer =
            I2npMessage::decode_short_transport(first.message_bytes(), MAX_I2NP_PAYLOAD_SIZE)
                .map_err(|e| DestinationTunnelError::Outbound(format!("{e:?}")))?;
        match outer.body() {
            I2npBody::TunnelData(_) => {}
            other => {
                return Err(DestinationTunnelError::Outbound(format!(
                    "publication outer is not TunnelData: {:?}",
                    other.message_type()
                )));
            }
        }
        let expected_first = role.established().first_hop_router().hash();
        if first.target() != i2pr_transport::PeerId::from_hash(expected_first) {
            return Err(DestinationTunnelError::Outbound(
                "publication delivery target is not the outbound first hop".to_owned(),
            ));
        }
        if let Some(entry) = self.publications.get_mut(&request_id) {
            entry.message_id = message_id;
        }
        let proof = crate::netdb_tunnels::PublicationPathProof {
            cell_count: dispatch.cell_count,
            first_hop: expected_first,
            floodfill: target_floodfill,
            via_tunnel: true,
        };
        Ok((dispatch, proof))
    }

    /// Correlates a `DeliveryStatus` reply with a tracked local
    /// LeaseSet2 publication. A mismatched token is a typed rejection;
    /// only protocol-derived acknowledgement completes the row.
    pub fn correlate_ls2_publication_status(
        &mut self,
        message_id: u32,
    ) -> Result<u64, DestinationTunnelError> {
        let matched = self
            .publications
            .iter()
            .find_map(|(id, pending)| {
                if pending.message_id == message_id && message_id != 0 {
                    Some(*id)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                DestinationTunnelError::Publication("delivery status token mismatch".to_owned())
            })?;
        self.publications.remove(&matched);
        self.counters.publications_acked = self.counters.publications_acked.saturating_add(1);
        Ok(matched)
    }

    /// Cancels a local LeaseSet2 publication attempt.
    pub fn cancel_ls2_publication(
        &mut self,
        request_id: u64,
    ) -> Result<(), DestinationTunnelError> {
        self.publications
            .remove(&request_id)
            .map(|_| ())
            .ok_or(DestinationTunnelError::UnknownPublication)
    }

    /// Retries a local LeaseSet2 publication attempt without
    /// re-signing: the stored signed bytes are reused verbatim.
    pub fn retry_ls2_publication(&mut self, request_id: u64) -> Result<(), DestinationTunnelError> {
        if !self.publications.contains_key(&request_id) {
            return Err(DestinationTunnelError::UnknownPublication);
        }
        Ok(())
    }

    /// Recovers one destination `Garlic` envelope through the
    /// supplied registry (Plan 187 §7).
    ///
    /// The cell must traverse an activated real inbound endpoint;
    /// unknown tunnel ids never allocate state. The returned bytes
    /// are the standard-encoded Garlic carrier for the existing
    /// client-layer ECIES dispatch; this helper never decrypts.
    pub fn recover_garlic_bytes(
        &mut self,
        registry: &mut DataPlaneRegistry,
        cell: &i2pr_proto::TunnelDataMessage,
        now_ms: u64,
    ) -> Result<Vec<u8>, DestinationTunnelError> {
        let outcome = crate::inbound_dispatch::dispatch_inbound_tunnel_data(registry, cell, now_ms)
            .map_err(|error| match error {
                crate::inbound_dispatch::InboundDispatchError::UnknownTunnelId(id) => {
                    DestinationTunnelError::UnknownInboundTunnel(id)
                }
                crate::inbound_dispatch::InboundDispatchError::NoActiveInbound(id) => {
                    DestinationTunnelError::UnknownInboundTunnel(id)
                }
                other => DestinationTunnelError::Inbound(other.to_string()),
            })?;
        match outcome {
            crate::inbound_dispatch::InboundDispatchOutcome::GarlicComplete { bytes } => {
                self.counters.garlic_recovered = self.counters.garlic_recovered.saturating_add(1);
                Ok(bytes)
            }
            crate::inbound_dispatch::InboundDispatchOutcome::CellAccepted => {
                Err(DestinationTunnelError::CellIncomplete)
            }
            crate::inbound_dispatch::InboundDispatchOutcome::DatabaseStoreComplete { .. }
            | crate::inbound_dispatch::InboundDispatchOutcome::DatabaseSearchReplyComplete {
                ..
            }
            | crate::inbound_dispatch::InboundDispatchOutcome::DeliveryStatusComplete { .. } => {
                Err(DestinationTunnelError::UnexpectedBodyType { type_byte: 0 })
            }
        }
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
        let coord = DestinationTunnelCoordinator::new(
            LookupPolicy::default(),
            RouterInfoStoreConfig::default(),
        );
        assert_eq!(coord.store().len(), 0);
        assert_eq!(coord.floodfill_count(), 0);
        assert_eq!(coord.pending_len(), 0);
        assert_eq!(coord.publications_len(), 0);
    }

    #[test]
    fn direct_transport_is_never_counted() {
        let mut coord = DestinationTunnelCoordinator::new(
            LookupPolicy::default(),
            RouterInfoStoreConfig::default(),
        );
        let error = coord
            .note_direct_transport_attempt()
            .expect_err("direct must reject");
        assert_eq!(error, DestinationTunnelError::DirectTransportRejected);
        assert_eq!(coord.counters().direct_rejections, 1);
    }
}
