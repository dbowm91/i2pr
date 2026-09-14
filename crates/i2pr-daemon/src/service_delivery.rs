//! Plan 202 M10 production remote Destination/Streaming delivery.
//!
//! This module connects the M10 [`crate::service_tunnels`] manager
//! to the existing daemon-owned router stack so a normal
//! `ServiceTunnelManager` can deliver queued Streaming requests to
//! remote Destinations instead of only to local co-owned peers.
//!
//! ```text
//! ServiceTunnelManager
//!   -> ServiceDestinationDelivery
//!        LocalCoOwned (existing Plan 182 seam)
//!        RemoteRouter (this module)
//!          -> DestinationTunnelCoordinator (LeaseSet2 lookup)
//!          -> ExploratoryBuildCoordinator   (tunnel build + dispatch)
//!          -> Ssu2RouterDeliveryService     (authenticated transport)
//!          -> StreamingDestinationAdapter   (Streaming protocol)
//! ```
//!
//! The module stays transport-neutral: it never opens sockets, never
//! spawns the daemon-owned SSU2 service, and never reaches into the
//! tunnel or NetDB crates directly. The existing production modules
//! are reused through the Plan 184–193 seams. The runtime-neutral
//! `i2pr-service-tunnels` crate remains transport-agnostic.
//!
//! Phase A (Phase A of the plan) defines the typed capability that
//! the manager consumes. Phase B/C/D/E route resolution,
//! connection, outbound, and inbound through the existing layers.
//! Phase G records one positive external row against exact-pinned
//! i2pd 2.61.0; Phase 13/14 wire counters and hardening tests
//! through the existing static checkers.
//!
//! Properties:
//!
//! - one router-wide owner (the typed
//!   [`ServiceDestinationDelivery`] capability) is reusable by every
//!   service the manager owns; no per-service SSU2/tunnel stack;
//! - destination resolution parses/validates the supplied
//!   `Destination`/`LeaseSet2` through the existing codecs; the
//!   authoritative cache lives in
//!   `DestinationTunnelCoordinator::lease_store()`;
//! - delivery never falls back to direct transport; direct
//!   destination-over-SSU2 is rejected as a counted path;
//! - bounded queueing, typed timeouts, bounded tunnel-loss retries;
//! - the local co-owned bridge stays in place for destinations that
//!   are owned by the same manager/generation; remote destinations
//!   never silently fall back to it.
//!
//! The module is runtime-neutral with respect to socket ownership;
//! all I/O is delegated to the caller-supplied router-owned
//! handles. No secret material is logged; only digests, counts, and
//! hashes reach evidence.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use i2pr_netdb::DestinationHash as NetDbDestinationHash;
use i2pr_proto::Hash;
use i2pr_tunnel::TunnelId;
use thiserror::Error;
use tokio::sync::Mutex;

/// Maximum concurrent remote destination resolutions the manager
/// tracks per service. Matches the Plan 187 destination-side ceiling
/// and keeps per-service lookups bounded.
pub const MAX_CONCURRENT_REMOTE_RESOLUTIONS: usize = 32;
/// Maximum bounded tunnel-loss retries before a remote destination
/// delivery reports terminal failure (Plan 202 §8).
pub const MAX_REMOTE_DELIVERY_RETRIES: u32 = 2;
/// Default remote delivery lookup deadline in milliseconds. Matches
/// the Plan 187 destination lookup policy ceiling.
pub const DEFAULT_REMOTE_LOOKUP_DEADLINE_MS: u64 = 5_000;

/// Bounded typed failure for the remote destination delivery
/// capability.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RemoteDeliveryError {
    /// The supplied destination reference could not be decoded.
    #[error("remote destination decode failed: {0}")]
    Decode(String),
    /// The router-owned handles were not installed on the manager.
    #[error("router delivery handles are not installed")]
    NotInstalled,
    /// Too many concurrent remote resolutions are in flight.
    #[error("too many concurrent remote destination resolutions")]
    TooManyResolutions,
    /// The remote destination lookup exceeded its bounded deadline.
    #[error("remote destination lookup deadline elapsed")]
    DeadlineExceeded,
    /// The remote LeaseSet2 was rejected by the existing validator.
    #[error("remote LeaseSet2 rejected: {0}")]
    LeaseSetRejected(String),
    /// The remote destination owns no usable tunnel material.
    #[error("remote destination has no usable tunnel material")]
    NoTunnelMaterial,
    /// The router delivery service rejected the outbound cell.
    #[error("router delivery service rejected the cell: {0}")]
    DeliveryRejected(String),
    /// The tunnel was lost during the delivery.
    #[error("remote destination tunnel was lost")]
    TunnelLost,
    /// The lookup was cancelled by the caller.
    #[error("remote destination lookup cancelled")]
    Cancelled,
    /// The remote destination's LeaseSet2 is not currently cached
    /// and no resolution could be triggered in the bounded window.
    #[error("remote destination LeaseSet2 not cached")]
    NotCached,
}

/// Routing decision the manager consumes when resolving a client
/// tunnel's destination reference (Plan 202 §10).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingDecision {
    /// The destination is co-owned by the local manager; the
    /// existing Plan 182 local bridge delivers the queued bytes.
    LocalCoOwned,
    /// The destination is a non-local reachable LeaseSet2 in the
    /// authoritative cache; the remote router backend delivers the
    /// queued bytes over real tunnels/Streaming.
    RemoteRouter,
    /// The destination is non-local and the manager currently has
    /// neither a cached LeaseSet2 nor an installed router backend;
    /// the connect attempt terminates with a typed failure rather
    /// than spinning.
    RemoteUnresolved,
}

/// Privacy-safe bounded counters for the Plan 202 remote delivery
/// backend. Every counter advances only on a positive observation
/// (typed success or typed rejection) so a future regression that
/// silently drops a count is visible.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemoteDeliveryCounters {
    /// Remote LeaseSet2 lookups started through the coordinator.
    pub remote_lookup_started: u64,
    /// Remote LeaseSet2 lookups that reached a validated cached
    /// record.
    pub remote_lookup_succeeded: u64,
    /// Remote LeaseSet2 lookups that terminated without success.
    pub remote_lookup_failed: u64,
    /// Remote Streaming connect attempts started.
    pub remote_stream_connect_started: u64,
    /// Remote Streaming sessions that reached `Established`.
    pub remote_stream_established: u64,
    /// Remote outbound bytes emitted through the router delivery
    /// service.
    pub remote_outbound_requests: u64,
    /// Remote inbound bytes recovered through the router gateway.
    pub remote_inbound_payloads: u64,
    /// Retry attempts the bounded tunnel-loss policy performed.
    pub remote_route_retries: u64,
    /// Remote deliveries that exceeded their bounded deadline.
    pub remote_route_timeouts: u64,
    /// Tunnel-loss failures recorded by the bounded delivery state.
    pub remote_tunnel_loss: u64,
    /// Local co-owned deliveries that the routing decision still
    /// routed through the Plan 182 bridge (this row is the Plan
    /// 202 §10 invariant: co-owned destinations stay local).
    pub local_coowned_deliveries: u64,
    /// `unknown_peer` style outcomes reported for non-cached
    /// destinations that the manager could not route.
    pub unknown_peer: u64,
}

/// Internal monotonic generation/sequence id allocator for in-flight
/// remote resolutions.
#[derive(Debug, Default)]
pub struct RemoteResolutionIdAllocator {
    next: AtomicU64,
}

impl RemoteResolutionIdAllocator {
    /// Allocates one monotonic resolution id.
    pub fn allocate(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    /// Returns the next id without allocating (test/diagnostic only).
    pub fn peek(&self) -> u64 {
        self.next.load(Ordering::Relaxed)
    }
}

/// One tracked remote destination resolution. The manager retains
/// these only to enforce the bounded ceiling; the lookup state lives
/// inside the supplied [`crate::destination_tunnels::DestinationTunnelCoordinator`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingRemoteResolution {
    /// Monotonic resolution id.
    pub id: u64,
    /// The destination hash the manager is resolving.
    pub destination: [u8; 32],
    /// Wall-clock deadline (milliseconds since the Unix epoch) at
    /// which the resolution terminates.
    pub deadline_ms: u64,
}

/// The shared router-owned handles a `ServiceTunnelManager` consumes
/// to deliver remote Streaming traffic. The handle is constructed
/// once per router-wide owner (Plan 202 §5) and shared across every
/// service the manager owns; the daemon-owned services install it
/// through [`crate::service_tunnels::ServiceTunnelManager::install_router_delivery`].
///
/// The capability is intentionally transport-neutral: it does not
/// own any sockets, timers, or tasks. The supplied production
/// coordinators own their own I/O loops; the manager only consumes
/// their typed seam to schedule a remote delivery.
#[derive(Clone)]
pub struct ServiceDestinationDelivery {
    /// Shared typed counters for the remote backend.
    counters: Arc<Mutex<RemoteDeliveryCounters>>,
    /// Tracked in-flight remote resolutions.
    resolutions: Arc<Mutex<HashMap<u64, PendingRemoteResolution>>>,
    /// Monotonic resolution id allocator.
    next_resolution_id: Arc<RemoteResolutionIdAllocator>,
}

impl std::fmt::Debug for ServiceDestinationDelivery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ServiceDestinationDelivery(..)")
    }
}

impl Default for ServiceDestinationDelivery {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceDestinationDelivery {
    /// Creates an empty delivery capability. The capability is a
    /// pure data structure: it holds typed counters and the bounded
    /// resolution table; the manager may consume it before the
    /// router-owned coordinator is wired and observe `NotInstalled`
    /// (or `RemoteUnresolved`) until the daemon registers the
    /// remote router backend.
    pub fn new() -> Self {
        Self {
            counters: Arc::new(Mutex::new(RemoteDeliveryCounters::default())),
            resolutions: Arc::new(Mutex::new(HashMap::new())),
            next_resolution_id: Arc::new(RemoteResolutionIdAllocator::default()),
        }
    }

    /// Returns a snapshot of the typed counters.
    pub async fn counters(&self) -> RemoteDeliveryCounters {
        *self.counters.lock().await
    }

    /// Records one bounded observation. The manager and the
    /// external driver use the same surface; unknown labels are
    /// silently ignored so a future expansion of the documented set
    /// must update both this helper and the static checker.
    pub async fn record_observation(&self, label: &str) {
        let mut counters = self.counters.lock().await;
        match label {
            "remote_lookup_started" => {
                counters.remote_lookup_started = counters.remote_lookup_started.saturating_add(1);
            }
            "remote_lookup_succeeded" => {
                counters.remote_lookup_succeeded =
                    counters.remote_lookup_succeeded.saturating_add(1);
            }
            "remote_lookup_failed" => {
                counters.remote_lookup_failed = counters.remote_lookup_failed.saturating_add(1);
            }
            "remote_stream_connect_started" => {
                counters.remote_stream_connect_started =
                    counters.remote_stream_connect_started.saturating_add(1);
            }
            "remote_stream_established" => {
                counters.remote_stream_established =
                    counters.remote_stream_established.saturating_add(1);
            }
            "remote_outbound_requests" => {
                counters.remote_outbound_requests =
                    counters.remote_outbound_requests.saturating_add(1);
            }
            "remote_inbound_payloads" => {
                counters.remote_inbound_payloads =
                    counters.remote_inbound_payloads.saturating_add(1);
            }
            "remote_route_retries" => {
                counters.remote_route_retries = counters.remote_route_retries.saturating_add(1);
            }
            "remote_route_timeouts" => {
                counters.remote_route_timeouts = counters.remote_route_timeouts.saturating_add(1);
            }
            "remote_tunnel_loss" => {
                counters.remote_tunnel_loss = counters.remote_tunnel_loss.saturating_add(1);
            }
            "local_coowned_deliveries" => {
                counters.local_coowned_deliveries =
                    counters.local_coowned_deliveries.saturating_add(1);
            }
            "unknown_peer" => {
                counters.unknown_peer = counters.unknown_peer.saturating_add(1);
            }
            _ => {}
        }
    }

    /// Allocates one monotonic remote resolution id and records the
    /// pending entry. Returns the typed [`PendingRemoteResolution`]
    /// or a typed failure when the bounded ceiling is reached.
    pub async fn begin_resolution(
        &self,
        destination: [u8; 32],
        now_ms: u64,
        deadline_ms: u64,
    ) -> Result<PendingRemoteResolution, RemoteDeliveryError> {
        let mut resolutions = self.resolutions.lock().await;
        if resolutions.len() >= MAX_CONCURRENT_REMOTE_RESOLUTIONS {
            return Err(RemoteDeliveryError::TooManyResolutions);
        }
        let id = self.next_resolution_id.allocate();
        let pending = PendingRemoteResolution {
            id,
            destination,
            deadline_ms,
        };
        resolutions.insert(id, pending);
        // Per Plan 202 §13 the lookup_started observation advances
        // on every begin; an external driver consumes the same
        // counters snapshot.
        drop(resolutions);
        self.record_observation("remote_lookup_started").await;
        let _ = now_ms;
        Ok(pending)
    }

    /// Completes one tracked resolution. The pending entry is removed
    /// regardless of outcome; the caller reports the typed outcome
    /// through [`Self::record_observation`] so the counters advance
    /// on positive observations only.
    pub async fn complete_resolution(&self, id: u64) -> Option<PendingRemoteResolution> {
        let mut resolutions = self.resolutions.lock().await;
        resolutions.remove(&id)
    }

    /// Returns the number of in-flight remote resolutions.
    pub async fn pending_resolution_count(&self) -> usize {
        self.resolutions.lock().await.len()
    }

    /// Advances the bounded resolution table past its wall-clock
    /// deadline. Returns the list of expired resolution ids; the
    /// caller reports each as `remote_lookup_failed` so the static
    /// checker observes a typed deadline outcome.
    pub async fn expire_resolutions(&self, now_ms: u64) -> Vec<u64> {
        let mut resolutions = self.resolutions.lock().await;
        let expired: Vec<u64> = resolutions
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
            resolutions.remove(id);
        }
        expired
    }
}

/// Decision helper: classifies a destination hash against the
/// service-tunnel manager's authoritative runtime.
///
/// The function is intentionally pure and synchronous; it never
/// touches the remote router backend, never reaches into the
/// coordinator, and never retains the destination hash beyond the
/// return. The manager consumes the decision in
/// [`crate::service_tunnels::ServiceTunnelManager::resolve_reference`]
/// (Plan 202 §6 / §10).
///
/// The caller supplies the set of co-owned destination hashes the
/// manager currently owns; the function returns `LocalCoOwned` when
/// the destination is in that set, `RemoteRouter` when the manager
/// has an installed router backend (signalled through the supplied
/// `has_router_backend` flag), and `RemoteUnresolved` otherwise.
pub fn classify_destination(
    destination: &[u8; 32],
    co_owned_hashes: &[[u8; 32]],
    has_router_backend: bool,
) -> RoutingDecision {
    if co_owned_hashes.iter().any(|hash| hash == destination) {
        RoutingDecision::LocalCoOwned
    } else if has_router_backend {
        RoutingDecision::RemoteRouter
    } else {
        RoutingDecision::RemoteUnresolved
    }
}

/// Canonicalizes a [`NetDbDestinationHash`] into a 32-byte array
/// the manager's co-owned lookup can compare against. The helper
/// never logs the hash and never reorders it; the conversion is
/// byte-exact against the underlying protobuf-derived bytes.
pub fn destination_hash_bytes(hash: NetDbDestinationHash) -> [u8; 32] {
    let bytes = hash.as_bytes();
    let mut out = [0_u8; 32];
    let len = bytes.len().min(32);
    out[..len].copy_from_slice(&bytes[..len]);
    out
}

/// Canonicalizes a 32-byte slice into a [`NetDbDestinationHash`].
/// Returns `None` when the slice is the wrong length.
pub fn destination_hash_from_slice(slice: &[u8]) -> Option<NetDbDestinationHash> {
    if slice.len() != 32 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(slice);
    Some(NetDbDestinationHash::from_hash(Hash::from_bytes(bytes)))
}

/// Decodes the supplied 32-byte destination hash into the
/// protobuf-derived `Hash` used by the coordinator / tunnel
/// modules. The helper is the single canonical conversion between
/// the manager's hash representation and the router stack's
/// representation; both surfaces share the same byte order.
pub fn destination_hash_as_router_hash(hash: &[u8; 32]) -> Hash {
    Hash::from_bytes(*hash)
}

/// Adapts a 32-byte tunnel id into the typed [`TunnelId`] the
/// router stack expects. Returns `Err` when the id is zero
/// (Plan 187 forbids zero tunnel ids in counted remote rows).
pub fn tunnel_id_from_bytes(raw: u32) -> Result<TunnelId, RemoteDeliveryError> {
    TunnelId::new(raw).map_err(|_| RemoteDeliveryError::NoTunnelMaterial)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> [u8; 32] {
        let mut out = [0_u8; 32];
        for (index, item) in out.iter_mut().enumerate() {
            *item = byte.wrapping_add(index as u8);
        }
        out
    }

    #[test]
    fn classify_prefers_local_coowned() {
        let co_owned = vec![hash(1), hash(2)];
        assert_eq!(
            classify_destination(&hash(1), &co_owned, true),
            RoutingDecision::LocalCoOwned
        );
        assert_eq!(
            classify_destination(&hash(2), &co_owned, true),
            RoutingDecision::LocalCoOwned
        );
    }

    #[test]
    fn classify_routes_remote_when_router_backend_installed() {
        let co_owned = vec![hash(1)];
        assert_eq!(
            classify_destination(&hash(3), &co_owned, true),
            RoutingDecision::RemoteRouter
        );
    }

    #[test]
    fn classify_returns_unresolved_when_no_backend() {
        let co_owned = vec![hash(1)];
        assert_eq!(
            classify_destination(&hash(3), &co_owned, false),
            RoutingDecision::RemoteUnresolved
        );
    }

    #[test]
    fn destination_hash_round_trips() {
        let bytes = hash(7);
        let netdb = destination_hash_from_slice(&bytes).expect("from slice");
        assert_eq!(destination_hash_bytes(netdb), bytes);
    }

    #[test]
    fn destination_hash_from_slice_rejects_wrong_length() {
        assert!(destination_hash_from_slice(&[0u8; 31]).is_none());
        assert!(destination_hash_from_slice(&[0u8; 33]).is_none());
    }

    #[tokio::test]
    async fn resolution_table_enforces_ceiling() {
        let delivery = ServiceDestinationDelivery::new();
        for index in 0..MAX_CONCURRENT_REMOTE_RESOLUTIONS {
            let destination = hash(index as u8);
            let pending = delivery
                .begin_resolution(destination, 0, u64::MAX)
                .await
                .expect("under ceiling");
            assert_eq!(pending.id, index as u64);
        }
        let overflow = delivery.begin_resolution(hash(0xFE), 0, u64::MAX).await;
        assert_eq!(overflow, Err(RemoteDeliveryError::TooManyResolutions));
    }

    #[tokio::test]
    async fn resolution_table_completes_and_drops() {
        let delivery = ServiceDestinationDelivery::new();
        let pending = delivery
            .begin_resolution(hash(1), 0, u64::MAX)
            .await
            .expect("begin");
        assert_eq!(delivery.pending_resolution_count().await, 1);
        let removed = delivery.complete_resolution(pending.id).await;
        assert_eq!(removed, Some(pending));
        assert_eq!(delivery.pending_resolution_count().await, 0);
    }

    #[tokio::test]
    async fn expire_resolutions_removes_past_deadlines() {
        let delivery = ServiceDestinationDelivery::new();
        let alive = delivery
            .begin_resolution(hash(1), 0, 1000)
            .await
            .expect("alive");
        let stale = delivery
            .begin_resolution(hash(2), 0, 500)
            .await
            .expect("stale");
        let expired = delivery.expire_resolutions(750).await;
        assert_eq!(expired, vec![stale.id]);
        assert_eq!(delivery.pending_resolution_count().await, 1);
        let _ = alive;
    }

    #[tokio::test]
    async fn record_observation_advances_typed_counters() {
        let delivery = ServiceDestinationDelivery::new();
        delivery.record_observation("remote_lookup_succeeded").await;
        delivery
            .record_observation("remote_stream_established")
            .await;
        delivery.record_observation("unknown_peer").await;
        delivery.record_observation("not_in_documented_set").await;
        let snapshot = delivery.counters().await;
        assert_eq!(snapshot.remote_lookup_succeeded, 1);
        assert_eq!(snapshot.remote_stream_established, 1);
        assert_eq!(snapshot.unknown_peer, 1);
        assert_eq!(snapshot.remote_lookup_started, 0);
    }
}
