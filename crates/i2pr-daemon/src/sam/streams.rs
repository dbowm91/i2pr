//! Plan 138 / Plan 139 / Plan 143 SAM 3.1 STREAM bridge runtime
//! surface.
//!
//! Plan 143 replaces the captured-outbound test seam with the
//! full Plan 129 local destination product path. Each SAM
//! destination owns one [`SamDestinationBridge`] backed by the
//! production destination runtime: signed LeaseSet2, signed
//! ECIES session manager, destination dispatcher, destination
//! routing, the `StreamingManager`, and the
//! outbound tunnel role. The bridge keeps the per-stream task
//! lock brief and replaces the plan-138 `record_captured` /
//! `adapter_send` test seam with a real
//! [`i2pr_client::deliver`] call.
//!
//! Every [`SamDestinationBridge`] pairs with a peer's bridge
//! through [`SamDestinations`]: when the SAM STREAM bridge
//! issues a `StreamingManager::connect`, the resulting
//! `TransportSendRequest` is routed through the per-pair
//! `LocalDeliveryInputs` to the peer's bridge, which feeds
//! `StreamingManager::accept_inbound_syn` and reverse-routes
//! the SYN response back. The same path crosses the full
//! destination stack on every steady-state send and on every
//! retransmit / ACK poll.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use i2pr_client::bundle::ReplyBundling;
use i2pr_client::streaming::config::StreamingConfig;
use i2pr_client::streaming::manager::StreamingManager;
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::{
    BatchedAttempt, DestinationDispatcher, DestinationId, DestinationIdentity,
    DestinationOutboundRole, DestinationRouting, DestinationRoutingConfig, EciesSessionConfig,
    EciesSessionManager, LeaseSetError, LocalDeliveryError, LocalDeliveryOutcome,
    LocalDeliveryReceiver, LocalDeliverySender, StreamingDestinationAdapter, deliver,
    deliver_batched,
};
use i2pr_crypto::OsRng;
use i2pr_netdb::LeaseSet2Store;
use i2pr_netdb::{LeaseSet2ValidationContext, ValidatedLeaseSet2};
use i2pr_proto::Hash;
use i2pr_proto::LeaseSet2;
use i2pr_proto::streaming::{FLAG_SYNCHRONIZE, peek_streaming_header};
use i2pr_proto::{
    I2npBody, I2npMessage, MAX_I2NP_PAYLOAD_SIZE, PROTOCOL_TYPE_STREAMING, decode_i2cp_data_body,
};
use i2pr_tunnel::{EstablishedTunnel, TunnelId, pool::TunnelSlot};
use rand_core::{CryptoRng, RngCore, UnwrapMut};

use crate::sam::SamServiceError;

/// Hard ceiling on the retained outbound queue the per-destination
/// runtime surfaces for diagnostics. The seam is the production
/// adapter path, not a captured-outbound queue, so this ceiling is
/// only a safety belt under failure.
pub const MAX_BRIDGE_DIAGNOSTIC_QUEUE: usize = 1024;

/// Plan 143 removed the captured-outbound test seam. The diagnostic
/// counter below preserves bounded observability without retaining
/// application payloads or destination-bearing transport requests.
#[derive(Clone, Debug, Default)]
pub struct BridgeDiagnostics {
    outbound_dispatches: usize,
    inbound_dispatched: u64,
    inbound_observations: u64,
    /// Garlic envelopes that authenticated, cumulative over every
    /// router-inbound dispatch producing a report (Plan 478
    /// garlic-arrival attribution).
    garlic_authenticated: u64,
    /// Garlic envelopes rejected at the authenticated-dispatch
    /// boundary, cumulative (category detail stays in the
    /// per-dispatch report and the product snapshot).
    garlic_rejected: u64,
    /// Destination payloads dequeued across authenticated dispatches.
    payloads_dequeued: u64,
    /// Dequeued payloads accepted into Streaming.
    streaming_accepted: u64,
}

impl BridgeDiagnostics {
    fn new() -> Self {
        Self {
            outbound_dispatches: 0,
            inbound_dispatched: 0,
            inbound_observations: 0,
            garlic_authenticated: 0,
            garlic_rejected: 0,
            payloads_dequeued: 0,
            streaming_accepted: 0,
        }
    }

    fn record_outbound(&mut self, _request: TransportSendRequest) {
        self.outbound_dispatches = self
            .outbound_dispatches
            .saturating_add(1)
            .min(MAX_BRIDGE_DIAGNOSTIC_QUEUE);
    }

    fn record_inbound_dispatch(&mut self) {
        self.inbound_dispatched = self.inbound_dispatched.saturating_add(1);
    }

    fn record_inbound_observation(&mut self) {
        self.inbound_observations = self.inbound_observations.saturating_add(1);
    }

    pub fn outbound_queue_len(&self) -> usize {
        self.outbound_dispatches
    }

    pub fn inbound_dispatched(&self) -> u64 {
        self.inbound_dispatched
    }

    pub fn inbound_observations(&self) -> u64 {
        self.inbound_observations
    }

    /// Plan 478 garlic-arrival attribution: folds one router-inbound
    /// dispatch report into the cumulative outcome counters. Counts
    /// only — no identities, keys, payloads, or rejection strings.
    /// Existing `inbound_dispatched` / `inbound_observations` calls
    /// stay untouched; this adds the outcome breakdown they lack.
    pub fn record_dispatch_report(&mut self, report: &RouterInboundDispatchReport) {
        if report.garlic_authenticated {
            self.garlic_authenticated = self.garlic_authenticated.saturating_add(1);
            self.payloads_dequeued = self
                .payloads_dequeued
                .saturating_add(report.payloads_dequeued as u64);
            self.streaming_accepted = self
                .streaming_accepted
                .saturating_add(report.streaming_packets_accepted as u64);
        } else {
            self.garlic_rejected = self.garlic_rejected.saturating_add(1);
        }
    }

    /// Cumulative garlic-authenticated dispatch envelopes.
    pub fn garlic_authenticated(&self) -> u64 {
        self.garlic_authenticated
    }

    /// Cumulative garlic-rejected dispatch envelopes.
    pub fn garlic_rejected(&self) -> u64 {
        self.garlic_rejected
    }

    /// Cumulative payloads dequeued across authenticated dispatches.
    pub fn payloads_dequeued(&self) -> u64 {
        self.payloads_dequeued
    }

    /// Cumulative dequeued payloads accepted into Streaming.
    pub fn streaming_accepted(&self) -> u64 {
        self.streaming_accepted
    }
}

/// Plan 212 §4 — router-backed per-service Destination network
/// state.
///
/// `SamLocalProductFabric` remains the explicitly local/co-owned
/// seam (synthetic localhost-only tunnel material). Remote-capable
/// network state is a distinct optional attachment to the same
/// service Destination runtime:
///
/// - `outbound_role` originates from an installed
///   `ExploratoryBuildCoordinator` outbound role (never
///   `dummy_outbound_tunnel`, `random_outbound_tunnel`,
///   `LocalZeroHop`, or fabric material);
/// - `lease_set2` is signed by the service Destination identity
///   from actual inbound lease metadata (`InboundLeaseSource`);
/// - `inbound_receive_ids` come from the installed real inbound
///   tunnel registry;
/// - remote routing/session state attaches to the existing service
///   bridge/runtime; the existing canonical service
///   `StreamingManager` remains authoritative (no second manager).
///
/// No secret material is logged or exposed through diagnostics;
/// `SamDestinationBridge::router_network_summary` exposes only
/// service-neutral counts/ids/expiry.
pub struct RouterDestinationNetworkState {
    destination_id: DestinationId,
    routing: DestinationRouting,
    session_manager: EciesSessionManager,
    outbound_roles: Vec<GroupOutboundRole>,
    next_outbound_role: usize,
    lease_set2: Option<LeaseSet2>,
    #[allow(dead_code)]
    validated_lease_set2: Option<ValidatedLeaseSet2>,
    inbound_receive_ids: Vec<u32>,
    outbound_expires_at_ms: u64,
    inbound_expires_at_ms: u64,
}

/// Secret-bearing data-plane projection bound to its canonical group pool
/// registration. The group pool owns membership and public routing metadata;
/// this projection owns the activated role needed by composition.
struct GroupOutboundRole {
    pool_slot: TunnelSlot,
    role: DestinationOutboundRole,
}

impl std::fmt::Debug for RouterDestinationNetworkState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RouterDestinationNetworkState")
            .field("destination_id", &self.destination_id)
            .field("inbound_receive_ids", &self.inbound_receive_ids.len())
            .field("outbound_expires_at_ms", &self.outbound_expires_at_ms)
            .field("inbound_expires_at_ms", &self.inbound_expires_at_ms)
            .finish_non_exhaustive()
    }
}

impl RouterDestinationNetworkState {
    /// Returns the real receive ids bundled with this generation so its
    /// manager owner projection can be checked before replacing bridge state.
    pub(crate) fn inbound_receive_ids(&self) -> &[u32] {
        &self.inbound_receive_ids
    }

    /// Builds router-backed state from already-installed real
    /// material. The caller owns tunnel installation; this
    /// constructor only bundles the state and checks identity
    /// consistency at install time (see
    /// [`SamDestinationBridge::install_router_network_state`]).
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        destination_id: DestinationId,
        routing: DestinationRouting,
        session_manager: EciesSessionManager,
        outbound_role: DestinationOutboundRole,
        lease_set2: LeaseSet2,
        validated_lease_set2: ValidatedLeaseSet2,
        inbound_receive_ids: Vec<u32>,
        outbound_expires_at_ms: u64,
        inbound_expires_at_ms: u64,
    ) -> Self {
        Self {
            destination_id,
            routing,
            session_manager,
            outbound_roles: vec![GroupOutboundRole {
                pool_slot: TunnelSlot::from_raw(0),
                role: outbound_role,
            }],
            next_outbound_role: 0,
            lease_set2: Some(lease_set2),
            validated_lease_set2: Some(validated_lease_set2),
            inbound_receive_ids,
            outbound_expires_at_ms,
            inbound_expires_at_ms,
        }
    }

    /// Builds router-backed state with every usable outbound role owned by
    /// the same Destination group. The caller derives both role collections
    /// from that group's canonical pool registrations.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_outbound_roles(
        destination_id: DestinationId,
        routing: DestinationRouting,
        session_manager: EciesSessionManager,
        outbound_roles: Vec<(TunnelSlot, DestinationOutboundRole)>,
        lease_set2: LeaseSet2,
        validated_lease_set2: ValidatedLeaseSet2,
        inbound_receive_ids: Vec<u32>,
        outbound_expires_at_ms: u64,
        inbound_expires_at_ms: u64,
    ) -> Self {
        Self {
            destination_id,
            routing,
            session_manager,
            outbound_roles: outbound_roles
                .into_iter()
                .map(|(pool_slot, role)| GroupOutboundRole { pool_slot, role })
                .collect(),
            next_outbound_role: 0,
            lease_set2: Some(lease_set2),
            validated_lease_set2: Some(validated_lease_set2),
            inbound_receive_ids,
            outbound_expires_at_ms,
            inbound_expires_at_ms,
        }
    }

    /// Creates a bounded staging projection for a newly committed service
    /// Destination. It can collect activated roles while its first usable
    /// inbound lease and signed LS2 are still being provisioned.
    pub(crate) fn new_staged(destination_id: DestinationId) -> Self {
        Self {
            destination_id,
            routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            session_manager: EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound_roles: Vec::new(),
            next_outbound_role: 0,
            lease_set2: None,
            validated_lease_set2: None,
            inbound_receive_ids: Vec::new(),
            outbound_expires_at_ms: u64::MAX,
            inbound_expires_at_ms: u64::MAX,
        }
    }

    /// Returns true when either the outbound or inbound material is
    /// expired relative to `now_ms`.
    pub(crate) fn is_expired(&self, now_ms: u64) -> bool {
        if self.lease_set2.is_none() {
            return false;
        }
        self.outbound_roles
            .iter()
            .all(|binding| binding.role.expires_at_ms() <= now_ms)
            || now_ms >= self.inbound_expires_at_ms
    }
}

/// Plan 212 §14 — read-only diagnostic summary for router-backed
/// state. Exposes only service id-neutral counts/ids/expiry; never
/// secret material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterNetworkSummary {
    /// Local destination hash (public identity, not secret).
    pub destination_hash: [u8; 32],
    /// Number of registered inbound receive tunnel ids.
    pub inbound_receive_count: usize,
    /// Outbound expiry (ms).
    pub outbound_expires_at_ms: u64,
    /// Inbound expiry (ms).
    pub inbound_expires_at_ms: u64,
    /// Number of leases in the installed real local LS2.
    ///
    /// Plan 213 §E2 — the qualification driver derives the
    /// real-lease-count fact from this typed field instead of
    /// asserting a literal success.
    pub lease_count: usize,
    /// Whether every installed inbound receive id currently
    /// resolves to a registered inbound tunnel owner.
    ///
    /// Plan 213 §E2 — filled by the manager (which owns the
    /// inbound-owner registry), not by the bridge. The bridge
    /// leaves the default `false`; see
    /// `ServiceTunnelManager::service_router_network_summary`.
    pub inbound_owner_registered: bool,
    /// Number of installed receive ids with a live owner registration.
    /// This count is diagnostic only; `is_complete` continues to require
    /// the all-receives predicate above.
    pub inbound_owner_count: usize,
}

impl RouterNetworkSummary {
    /// Returns whether the normal SAM owner can safely publish this
    /// router-backed destination as active.
    pub const fn is_complete(&self) -> bool {
        self.inbound_receive_count > 0 && self.lease_count > 0 && self.inbound_owner_registered
    }
}

/// Plan 212 §14 — outcome of draining authenticated destination
/// payloads into canonical Streaming.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterInboundDispatchReport {
    /// Whether the Garlic envelope authenticated.
    pub garlic_authenticated: bool,
    /// Bounded class of a typed Garlic rejection. No error strings,
    /// peer identifiers, or ciphertext are retained.
    pub garlic_rejection: Option<RouterGarlicRejection>,
    /// Number of destination payloads dequeued via `pop_payload`.
    pub payloads_dequeued: usize,
    /// Number of payloads accepted by
    /// `StreamingDestinationAdapter::receive` into the canonical
    /// service `StreamingManager`.
    pub streaming_packets_accepted: usize,
    /// Number of inbound SYN-response (SYN-ACK) packets the target
    /// `StreamingManager` classified during this dispatch
    /// (arrival count, Plan 478 handshake attribution).
    pub synack_observed: usize,
    /// Of the observed SYN-responses, how many transitioned an
    /// outbound connection to Established.
    pub synack_accepted: usize,
    /// Number of payloads rejected by the adapter (unknown
    /// non-datagram protocols count as rejected; Garlic auth
    /// failures return `garlic_authenticated = false` with zero
    /// dequeued).
    pub streaming_rejected: usize,
    /// Number of repliable/raw datagram payloads authenticated
    /// and queued on the bridge datagram manager (Plan 291).
    pub datagrams_accepted: usize,
}

/// Privacy-safe categories for locating the authenticated-dispatch boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterGarlicRejection {
    Structural,
    SessionTag,
    EciesAuthentication,
    EciesOther,
    Payload,
    MissingSenderLeaseSet2,
    LeaseSet2Signature,
    LeaseSet2Freshness,
    LeaseSet2Destination,
    LeaseSet2KeyPolicy,
    LeaseSet2BlindedPolicy,
    LeaseSet2Crypto,
    LeaseSet2CryptoProtocol,
    LeaseSet2UnsupportedSignatureType(u16),
    LeaseSet2CryptoKey,
    LeaseSet2Size,
    SenderKeyMismatch,
    UnknownDestination,
    Other,
}

/// Per-destination SAM STREAM bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InboundReceiveProjectionError {
    RouterStateMissing,
    DuplicateReceiveId,
}

#[allow(dead_code)]
pub struct SamDestinationBridge {
    identity: Arc<DestinationIdentity>,
    lease_set2: Option<LeaseSet2>,
    streaming: StreamingManager,
    routing: DestinationRouting,
    session_manager: EciesSessionManager,
    outbound_role: Option<DestinationOutboundRole>,
    dispatcher: DestinationDispatcher,
    lease_set2_store: LeaseSet2Store,
    receiver_dispatcher: DestinationDispatcher,
    receiver_session: EciesSessionManager,
    receiver_routing: DestinationRouting,
    receiver_streaming: StreamingManager,
    /// Plan 291: connectionless repliable/raw datagram manager. A
    /// single manager serves both directions because datagrams
    /// carry no connection state (no initiator/mirror split).
    datagrams: i2pr_client::datagram::DatagramManager,
    receiver_lease_set2_store: LeaseSet2Store,
    receiver_now_seconds: u32,
    diagnostics: BridgeDiagnostics,
    /// Plan 147 §10: test-only inbound-tunnel factory used by the
    /// per-destination runtime driver to construct a fresh
    /// `EstablishedTunnel` per delivery. Production deployments
    /// install a real inbound-tunnel pool here.
    inbound_tunnel_factory: Option<Arc<dyn InboundTunnelFactory>>,
    /// Plan 212 §4/§6 — optional router-backed network state for
    /// remote-capable service Destinations. `None` for local-only
    /// / co-owned traffic (which keeps using `SamLocalProductFabric`
    /// material). Remote compose must explicitly select this state
    /// and fail closed when it is missing or expired; it must never
    /// fall back to the local fabric.
    router_network: Option<RouterDestinationNetworkState>,
    /// Plan 487 WP3 — one-shot latch for the live DATA-phase stall
    /// attribution. The close receipt cannot serve that purpose: by the
    /// time the pump finishes, the session teardown has already
    /// removed the destination bridge, so every delta reads zero by
    /// construction. The live receipt is emitted at most once per
    /// destination, while the state is still readable.
    data_phase_stall_reported: bool,
    /// Plan 487 WP3 — the monotonic time at which this destination's
    /// streaming connection first reached the established state, used
    /// to bound the stall window before the live attribution fires.
    established_since_ms: Option<u64>,
    /// Plan 487 WP3 — the number of application payload bytes the
    /// ACCEPT-side pump has actually handed to the application.
    ///
    /// This is the **delivery** stage's own counter: it separates "the
    /// DATA never reached a manager" from "the DATA reached the owning
    /// manager but the application never received it". Counts only.
    app_bytes_drained: u64,
}

impl std::fmt::Debug for SamDestinationBridge {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SamDestinationBridge")
            .field("identity", &self.identity.id())
            .field("lease_set2", &"<redacted>")
            .field("streaming", &"<redacted>")
            .field("diagnostics", &self.diagnostics)
            .finish_non_exhaustive()
    }
}

impl SamDestinationBridge {
    /// Builds the sender-side bridge plus the receiver-side mirror
    /// for one destination. The identity is moved into the bridge.
    pub fn new(
        identity: DestinationIdentity,
        lease_set2: LeaseSet2,
        outbound_role: DestinationOutboundRole,
        now_seconds: u32,
    ) -> Self {
        Self::with_shared_identity(
            Arc::new(identity),
            lease_set2,
            outbound_role,
            now_seconds,
            StreamingConfig::balanced(),
        )
    }

    /// Builds the sender-side bridge plus the receiver-side mirror for one
    /// destination using an existing `Arc<DestinationIdentity>` allocation.
    ///
    /// Plan 149 §3 Option A: the SAM service builds one secret allocation
    /// per logical destination and shares the `Arc` with the destination
    /// runtime. The bridge never reconstructs a second private identity.
    ///
    /// Plan 292: the streaming profile selects the window/ACK
    /// configuration for both managers (balanced default, interactive
    /// for latency-sensitive service tunnels).
    pub fn with_shared_identity(
        identity: Arc<DestinationIdentity>,
        lease_set2: LeaseSet2,
        outbound_role: DestinationOutboundRole,
        now_seconds: u32,
        streaming_config: StreamingConfig,
    ) -> Self {
        Self::build(
            identity,
            Some(lease_set2),
            Some(outbound_role),
            now_seconds,
            streaming_config,
        )
    }

    /// Builds a router-network bridge before its real LeaseSet and tunnel
    /// pool have been provisioned. This constructor carries no local-fabric
    /// material; router-network composition must install validated live
    /// state before the bridge can route traffic.
    #[allow(dead_code)] // consumed by the bounded SAM/product factory integration
    pub(crate) fn with_router_identity(
        identity: Arc<DestinationIdentity>,
        now_seconds: u32,
        streaming_config: StreamingConfig,
    ) -> Self {
        Self::build(identity, None, None, now_seconds, streaming_config)
    }

    fn build(
        identity: Arc<DestinationIdentity>,
        lease_set2: Option<LeaseSet2>,
        outbound_role: Option<DestinationOutboundRole>,
        now_seconds: u32,
        streaming_config: StreamingConfig,
    ) -> Self {
        let mut receiver_dispatcher = DestinationDispatcher::new();
        receiver_dispatcher
            .register_destination(identity.id())
            .expect("destination register");
        receiver_dispatcher
            .bind_destination_hash(identity.id(), identity.id().as_netdb_key())
            .expect("destination hash bind");
        // Plan 213 corrective: the canonical dispatcher serves the
        // router-backed inbound path
        // (`dispatch_router_garlic_to_canonical_streaming` drains
        // `pop_payload` from it). It needs the same local
        // registration, otherwise every decrypted clove fails
        // closed with `UnknownDestination` at delivery routing even
        // though authentication succeeded.
        let mut dispatcher = DestinationDispatcher::new();
        dispatcher
            .register_destination(identity.id())
            .expect("destination register");
        dispatcher
            .bind_destination_hash(identity.id(), identity.id().as_netdb_key())
            .expect("destination hash bind");
        Self {
            identity,
            lease_set2,
            streaming: StreamingManager::new(streaming_config.clone()),
            routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            session_manager: EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound_role,
            dispatcher,
            lease_set2_store: LeaseSet2Store::default(),
            receiver_dispatcher,
            receiver_session: EciesSessionManager::new(EciesSessionConfig::balanced()),
            receiver_routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            receiver_streaming: StreamingManager::new(streaming_config),
            datagrams: i2pr_client::datagram::DatagramManager::new(),
            receiver_lease_set2_store: LeaseSet2Store::default(),
            receiver_now_seconds: now_seconds,
            diagnostics: BridgeDiagnostics::new(),
            inbound_tunnel_factory: None,
            router_network: None,
            data_phase_stall_reported: false,
            established_since_ms: None,
            app_bytes_drained: 0,
        }
    }

    pub fn identity(&self) -> Arc<DestinationIdentity> {
        Arc::clone(&self.identity)
    }

    pub fn identity_id(&self) -> DestinationId {
        self.identity.id()
    }

    pub fn lease_set2(&self) -> Option<&LeaseSet2> {
        self.lease_set2.as_ref()
    }

    pub fn outbound_role(&self) -> Option<&DestinationOutboundRole> {
        self.outbound_role.as_ref()
    }

    /// Plan 208 — mutable accessor for the per-destination outbound
    /// role. Production compose paths (`bridge_to_peer` and the
    /// Plan 208 remote-route helper) own the swap-and-restore cycle
    /// that consumes the role by move; the accessor is only used
    /// inside the daemon, never exposed over the public API.
    pub fn outbound_role_mut(&mut self) -> Option<&mut DestinationOutboundRole> {
        self.outbound_role.as_mut()
    }

    pub fn identity_destination_hash(&self) -> [u8; 32] {
        *self.identity.id().as_hash().as_bytes()
    }

    pub fn streaming_mut(&mut self) -> &mut StreamingManager {
        &mut self.streaming
    }

    pub fn streaming(&self) -> &StreamingManager {
        &self.streaming
    }

    pub fn session_manager_mut(&mut self) -> &mut EciesSessionManager {
        &mut self.session_manager
    }

    pub fn routing(&self) -> &DestinationRouting {
        &self.routing
    }

    pub fn routing_mut(&mut self) -> &mut DestinationRouting {
        &mut self.routing
    }

    pub fn receiver_streaming_mut(&mut self) -> &mut StreamingManager {
        &mut self.receiver_streaming
    }

    /// Returns the bridge's receiver-mirror `StreamingManager`. The
    /// receiver mirror processes inbound SYN/SYN-ACK/DATA packets
    /// delivered through [`bridge_to_peer`]; the canonical outbound
    /// path uses [`Self::streaming_mut`].
    pub fn receiver_streaming(&self) -> &StreamingManager {
        &self.receiver_streaming
    }

    /// Returns the bridge's connectionless datagram manager. The
    /// manager authenticates inbound repliable datagrams and queues
    /// raw media events for the owning service runtime (Plan 291).
    pub fn datagrams(&self) -> &i2pr_client::datagram::DatagramManager {
        &self.datagrams
    }

    /// Mutable accessor for the datagram manager. Used by the
    /// delivery seams (`bridge_to_peer` swap-and-restore,
    /// router-backed sink) and the owning service runtime drain.
    pub fn datagrams_mut(&mut self) -> &mut i2pr_client::datagram::DatagramManager {
        &mut self.datagrams
    }

    /// Polls retransmission and delayed-ACK state for both manager
    /// halves. The manager methods enqueue their own transport
    /// requests; the daemon only needs to invoke them once per
    /// driver tick.
    pub fn poll_streaming_timers(&mut self, now_ms: u64) {
        let _ = self.streaming.poll_retransmits(now_ms);
        let _ = self.streaming.poll_acks(now_ms);
        let _ = self.receiver_streaming.poll_retransmits(now_ms);
        let _ = self.receiver_streaming.poll_acks(now_ms);
    }

    /// Plan 487 WP3 — records application payload bytes handed to the
    /// application by the pump.
    pub(crate) fn note_app_bytes_drained(&mut self, bytes: usize) {
        self.app_bytes_drained = self.app_bytes_drained.saturating_add(bytes as u64);
    }

    /// Plan 487 WP3 — records the monotonic time at which this
    /// destination's streaming connection first reached the
    /// established state. The first observation wins; a later
    /// re-establishment does not reset the window.
    pub(crate) fn note_established_at(&mut self, now_ms: u64) {
        if self.established_since_ms.is_none() {
            self.established_since_ms = Some(now_ms);
        }
    }

    /// Plan 487 WP3 — the live DATA-phase stall snapshot for one
    /// destination, or `None` when the destination is not stalled.
    ///
    /// A destination is *stalled* when the receiver mirror owns an
    /// established (ACCEPT-side) connection but **no DATA has ever
    /// reached that manager**. The snapshot is one-shot per
    /// destination: it is emitted at most once, while the bridge is
    /// still installed, so the four stages can be read.
    ///
    /// The stage counts are per-destination (each destination owns its
    /// own two managers), and each stage is read from its own counter.
    /// No stage is inferred from another.
    pub(crate) fn data_phase_stall_snapshot(
        &mut self,
        now_ms: u64,
        ack_egress: (u64, u64),
    ) -> Option<DataPhaseStallSnapshot> {
        if self.data_phase_stall_reported {
            return None;
        }
        // The handshake must have completed, and the data phase must
        // have had a bounded window in which to deliver.
        let established_since_ms = self.established_since_ms?;
        if now_ms.saturating_sub(established_since_ms) < DATA_PHASE_STALL_WINDOW_MS {
            return None;
        }
        if self.app_bytes_drained != 0 {
            // The application received payload bytes: the data phase
            // is not stalled, whatever else is true.
            return None;
        }
        self.data_phase_stall_reported = true;
        Some(self.build_data_phase_stall_snapshot(now_ms, ack_egress))
    }

    /// Plan 487 WP3 — the **terminal** live DATA-phase stall snapshot:
    /// the same four stages, read at the moment the data phase ends
    /// instead of after a bounded window.
    ///
    /// The window-based snapshot can only fire while the destination
    /// is still alive. A stalled data phase frequently ends sooner
    /// than the window (the session tears down, the peer gives up),
    /// and the session teardown removes the destination bridge — after
    /// which every counter reads zero by construction. This snapshot
    /// is therefore taken **before** the bridge is removed, and it
    /// classifies the phase that just ended:
    ///
    /// - an established connection existed, and
    /// - the application received no payload bytes, and
    /// - no DATA packet reached either manager.
    ///
    /// That is a stall by definition — the peer sent data, the data
    /// phase ended, and nothing arrived. One-shot per destination, the
    /// same latch as the window-based snapshot.
    pub(crate) fn terminal_data_phase_stall_snapshot(
        &mut self,
        now_ms: u64,
        ack_egress: (u64, u64),
    ) -> Option<DataPhaseStallSnapshot> {
        if self.data_phase_stall_reported {
            return None;
        }
        // The handshake must have completed: a destination that never
        // reached the established state never had a data phase.
        self.established_since_ms?;
        if self.app_bytes_drained != 0 {
            // The application received payload bytes: the data phase
            // was not stalled, whatever else is true.
            return None;
        }
        let mirror = self.receiver_streaming.data_packet_ingress();
        let canonical = self.streaming.data_packet_ingress();
        if mirror.data_arrived != 0 || canonical.data_arrived != 0 {
            // A DATA packet did reach a manager: the data phase was
            // not an arrival loss, so this is not a stall.
            return None;
        }
        self.data_phase_stall_reported = true;
        Some(self.build_data_phase_stall_snapshot(now_ms, ack_egress))
    }

    /// Plan 487 WP3 — reads the four DATA-phase stages from their own
    /// counters. Every stage is read from the counter that owns it; no
    /// stage is inferred from another.
    fn build_data_phase_stall_snapshot(
        &self,
        now_ms: u64,
        ack_egress: (u64, u64),
    ) -> DataPhaseStallSnapshot {
        let canonical = self.streaming.data_packet_ingress();
        let mirror_ack = self.receiver_streaming.ack_generation();
        let canonical_ack = self.streaming.ack_generation();
        let mirror = self.receiver_streaming.data_packet_ingress();
        DataPhaseStallSnapshot {
            now_ms,
            app_bytes_drained: self.app_bytes_drained,
            mirror_observed: mirror.observed,
            mirror_accepted: mirror.accepted,
            mirror_rejected: mirror.rejected,
            mirror_payload_bytes: mirror.payload_bytes_admitted,
            mirror_arrived: mirror.data_arrived,
            canonical_observed: canonical.observed,
            canonical_accepted: canonical.accepted,
            canonical_rejected: canonical.rejected,
            canonical_payload_bytes: canonical.payload_bytes_admitted,
            canonical_arrived: canonical.data_arrived,
            mirror_ack_generated: mirror_ack.generated,
            mirror_ack_transmitted: mirror_ack.transmitted,
            mirror_ack_no_ack_emitted: mirror_ack.no_ack_emitted,
            canonical_ack_generated: canonical_ack.generated,
            canonical_ack_transmitted: canonical_ack.transmitted,
            ack_egress_sent: ack_egress.0,
            ack_egress_dropped: ack_egress.1,
        }
    }

    /// Returns whether either manager currently owns an established
    /// connection.
    pub fn has_established_connection(&self) -> bool {
        self.streaming
            .iter_connections()
            .chain(self.receiver_streaming.iter_connections())
            .any(|connection| {
                matches!(
                    connection.state(),
                    i2pr_client::streaming::connection::ConnectionState::Established
                )
            })
    }

    pub fn receiver_routing_mut(&mut self) -> &mut DestinationRouting {
        &mut self.receiver_routing
    }

    pub fn receiver_lease_set2_store_mut(&mut self) -> &mut LeaseSet2Store {
        &mut self.receiver_lease_set2_store
    }

    pub fn diagnostics(&self) -> &BridgeDiagnostics {
        &self.diagnostics
    }

    pub fn record_outbound_dispatch(&mut self, request: TransportSendRequest) {
        self.diagnostics.record_outbound(request);
    }

    pub fn record_inbound_dispatch(&mut self) {
        self.diagnostics.record_inbound_dispatch();
    }

    pub fn record_inbound_observation(&mut self) {
        self.diagnostics.record_inbound_observation();
    }

    /// Installs the per-destination inbound-tunnel factory used by
    /// the Plan 147 runtime driver. The factory is called once per
    /// outbound `TransportSendRequest`; each call returns a fresh
    /// `EstablishedTunnel` because the Plan 129 local seam
    /// consumes its argument. Returns the previous factory, if any.
    pub fn install_inbound_tunnel_factory(
        &mut self,
        factory: Arc<dyn InboundTunnelFactory>,
    ) -> Option<Arc<dyn InboundTunnelFactory>> {
        let prior = self.inbound_tunnel_factory.take();
        self.inbound_tunnel_factory = Some(factory);
        prior
    }

    /// Returns a clone of the installed inbound-tunnel factory, if
    /// any. Used by the runtime driver to construct a fresh
    /// `EstablishedTunnel` per delivery.
    pub fn inbound_tunnel_factory(&self) -> Option<Arc<dyn InboundTunnelFactory>> {
        self.inbound_tunnel_factory.clone()
    }

    pub fn identity_netdb_key(&self) -> i2pr_netdb::DestinationHash {
        self.identity.id().as_netdb_key()
    }

    /// Whether this bridge carries the explicit localhost-only fabric
    /// material used by the co-owned delivery seam. Router-network bridges
    /// stay out of that route and use the shared daemon product owner.
    fn has_local_product_material(&self) -> bool {
        // `bridge_to_peer` receives the peer's inbound tunnel from the
        // caller, so this co-owned delivery seam needs only the sender's
        // signed LeaseSet2 and outbound role. The runtime driver separately
        // requires an inbound factory when it provisions repeated deliveries.
        self.lease_set2.is_some() && self.outbound_role.is_some()
    }

    /// Plan 210 §E — runs the canonical
    /// [`i2pr_client::StreamingDestinationAdapter::send`] against the
    /// bridge's **real** mutable state without swapping in any
    /// placeholder routing / session / outbound role. The borrow
    /// checker can split-borrow the three disjoint private fields
    /// here, inside the impl block, because the access patterns
    /// resolve to direct field reads/writes that the compiler can
    /// prove are disjoint. The pre-Plan-210 swap-and-restore
    /// pattern used `dummy_outbound_tunnel()` as a counted
    /// placeholder for `outbound_role`; that path is gone, so
    /// counted remote compose runs against the same
    /// `DestinationRouting` / `EciesSessionManager` /
    /// `DestinationOutboundRole` the service-owned
    /// `StreamingManager` already drives.
    ///
    /// Returns the adapter's `OutboundDeliveryPlan` on success and
    /// a typed error string on failure; the caller decides how to
    /// convert the string into the manager's typed
    /// `RemoteDeliveryError` so this module stays free of
    /// `service_delivery` dependencies.
    ///
    /// Retained for the local/co-owned path only. Counted remote
    /// traffic must use [`Self::compose_router_send`].
    #[allow(dead_code)]
    pub(crate) fn compose_adapter_send_owned_fields(
        &mut self,
        request: &TransportSendRequest,
        now_seconds: u32,
        now_ms: u64,
    ) -> Result<i2pr_client::OutboundDeliveryPlan, String> {
        let local_id = self.identity.id();
        let local_static_secret = *self.identity.static_secret_bytes();
        let local_lease_set2 = self
            .lease_set2
            .as_ref()
            .ok_or_else(|| "local LeaseSet2 material is not installed".to_owned())?
            .clone();
        let mut os_rng = OsRng;
        let mut rng = UnwrapMut(&mut os_rng);
        let routing = &self.routing;
        let session = &mut self.session_manager;
        let outbound = self
            .outbound_role
            .as_ref()
            .ok_or_else(|| "local outbound role is not installed".to_owned())?;
        i2pr_client::StreamingDestinationAdapter::send(
            request,
            routing,
            session,
            outbound,
            local_id,
            &local_static_secret,
            &local_lease_set2,
            now_seconds,
            now_ms,
            &mut rng,
        )
        .map_err(|error| error.to_string())
    }

    /// Plan 210 §G — decodes one recovered standard I2NP envelope
    /// recovered from a remote `TunnelData` cell, runs the bridge's
    /// canonical `DestinationDispatcher::dispatch_garlic_envelope`
    /// against the canonical `EciesSessionManager` + identity +
    /// `LeaseSet2Store`, and records the typed outcome on the
    /// bridge's diagnostic surface so the static checker observes
    /// the production operation rather than the legacy
    /// "consumes internally" silent drop.
    ///
    /// Returns:
    /// - `Ok(true)` when the dispatcher authenticated an inbound
    ///   session / accepted payloads for the canonical local
    ///   destination;
    /// - `Ok(false)` when the dispatcher rejected the envelope
    ///   (typed failure already recorded);
    /// - `Err(_)` when the supplied bytes were not a standard
    ///   I2NP `Garlic` envelope.
    ///
    /// The bridge observes the canonical streaming / outbound role
    /// / dispatcher surface rather than the receiver mirror; this
    /// is the path Plan 210 §G §7-§9 requires for remote-traffic
    /// dispatch. The `LeaseSet2Store` is held on the canonical side
    /// so any validated remote LeaseSet install follows the
    /// canonical outbound routing decision.
    ///
    /// Retained for backwards compatibility. Counted remote traffic
    /// must use
    /// [`Self::dispatch_router_garlic_to_canonical_streaming`],
    /// which drains `pop_payload` into the canonical
    /// `StreamingManager` and gates the inbound counter on actual
    /// Streaming acceptance.
    #[allow(dead_code)]
    pub(crate) fn dispatch_inbound_garlic_owned(
        &mut self,
        bytes: &[u8],
        now_seconds: u32,
    ) -> Result<bool, String> {
        let envelope =
            i2pr_proto::I2npMessage::decode_standard(bytes, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .map_err(|error| format!("decode_standard failed: {error:?}"))?;
        if !matches!(envelope.body(), i2pr_proto::I2npBody::Garlic(_)) {
            return Err("envelope is not a standard I2NP Garlic body".to_owned());
        }
        // Dispatch the envelope through the bridge's canonical
        // dispatcher using the canonical session_manager + identity
        // + lease_set2_store. The dispatcher authenticates the
        // ECIES session, classifies the inbound payload blocks
        // (Plan 127 order), and binds any validated remote
        // LeaseSet2 to the canonical outbound routing pipeline.
        let local_id = self.identity.id();
        let local_static_secret = *self.identity.static_secret_bytes();
        let outcome = self.dispatcher.dispatch_garlic_envelope(
            &mut self.session_manager,
            local_id,
            &local_static_secret,
            &self.identity.static_public_bytes(),
            now_seconds,
            &envelope,
            &mut self.lease_set2_store,
        );
        match outcome {
            i2pr_client::InboundDispatchOutcome::Rejected(_error) => {
                self.diagnostics.record_inbound_observation();
                Ok(false)
            }
            i2pr_client::InboundDispatchOutcome::NewSessionProcessed {
                validated_remote_lease_set2,
                ..
            } => {
                let _ = self
                    .routing
                    .install_remote_lease_set2(*validated_remote_lease_set2);
                self.diagnostics.record_inbound_dispatch();
                Ok(true)
            }
            i2pr_client::InboundDispatchOutcome::ExistingSessionProcessed { .. }
            | i2pr_client::InboundDispatchOutcome::NewSessionReplyProcessed { .. } => {
                self.diagnostics.record_inbound_dispatch();
                Ok(true)
            }
        }
    }

    /// Plan 210 §G — read-only accessor for the bridge's diagnostic
    /// surface. The production composition reads this value before
    /// and after a Garlic dispatch so the Plan 210 §14 tests can
    /// observe a typed counter delta.
    pub fn diagnostics_snapshot(&self) -> &BridgeDiagnostics {
        &self.diagnostics
    }

    /// Plan 212 §6 — installs router-backed network state onto the
    /// service bridge.
    ///
    /// Fail-closed rules:
    /// 1. material must belong to this bridge's Destination identity;
    /// 2. re-install without explicit replacement fails (call
    ///    [`Self::clear_router_network_state`] first, or
    ///    [`Self::replace_router_network_state`]);
    /// 3. installation never alters the local/co-owned bridge
    ///    material (`lease_set2` / `routing` / `session_manager` /
    ///    `outbound_role` stay untouched);
    /// 4. expired material is rejected.
    ///
    /// No secret material is logged.
    pub(crate) fn install_router_network_state(
        &mut self,
        material: RouterDestinationNetworkState,
        now_ms: u64,
    ) -> Result<(), String> {
        if material.destination_id != self.identity.id() {
            return Err("router network state destination identity mismatch".to_owned());
        }
        if self.router_network.is_some() {
            return Err(
                "router network state already installed (clear or replace first)".to_owned(),
            );
        }
        if material.is_expired(now_ms) {
            return Err("router network state already expired".to_owned());
        }
        if material.lease_set2.is_some() && material.inbound_receive_ids.is_empty() {
            return Err("router network state carries no inbound receive ids".to_owned());
        }
        if material.inbound_receive_ids.contains(&0) {
            return Err("router network state carries zero receive tunnel id".to_owned());
        }
        // Validate the bundled LS2 against the service identity so a
        // mismatched record cannot be installed silently. The
        // validation uses seconds derived from `now_ms`.
        let now_seconds = u32::try_from(now_ms / 1000).unwrap_or(u32::MAX);
        let expected_key = self.identity.id().as_netdb_key();
        if let Some(lease_set2) = &material.lease_set2 {
            ValidatedLeaseSet2::from_lease_set2(
                lease_set2.clone(),
                Some(expected_key),
                LeaseSet2ValidationContext::new(now_seconds),
            )
            .map_err(|error| format!("router network LS2 validation failed: {error:?}"))?;
        }
        self.router_network = Some(material);
        Ok(())
    }

    /// Plan 212 §6 — explicit replacement path. Fails when no state
    /// is installed or when the replacement belongs to a different
    /// Destination identity or is already expired.
    #[allow(dead_code)]
    pub(crate) fn replace_router_network_state(
        &mut self,
        material: RouterDestinationNetworkState,
        now_ms: u64,
    ) -> Result<Option<RouterDestinationNetworkState>, String> {
        if material.destination_id != self.identity.id() {
            return Err("router network state destination identity mismatch".to_owned());
        }
        if material.is_expired(now_ms) {
            return Err("router network state already expired".to_owned());
        }
        Ok(self.router_network.replace(material))
    }

    /// Plan 212 §6 — removes router-backed state. Returns the
    /// removed state, if any. Local/co-owned material is untouched.
    pub(crate) fn clear_router_network_state(&mut self) -> Option<RouterDestinationNetworkState> {
        self.router_network.take()
    }

    /// Plan 212 §6 — returns true when router-backed state is
    /// installed and unexpired at `now_ms`.
    pub(crate) fn has_router_network_state(&self, now_ms: u64) -> bool {
        self.router_network
            .as_ref()
            .is_some_and(|state| !state.is_expired(now_ms))
    }

    /// Plan 212 §6 — read-only diagnostic summary. Exposes only
    /// destination hash, receive tunnel ids count, expiry, and
    /// counts; never secret material.
    pub(crate) fn router_network_summary(&self) -> Option<RouterNetworkSummary> {
        let state = self.router_network.as_ref()?;
        Some(RouterNetworkSummary {
            destination_hash: *self.identity.id().as_hash().as_bytes(),
            inbound_receive_count: state.inbound_receive_ids.len(),
            outbound_expires_at_ms: state.outbound_expires_at_ms,
            inbound_expires_at_ms: state.inbound_expires_at_ms,
            lease_count: state
                .lease_set2
                .as_ref()
                .map_or(0, |lease_set| lease_set.leases().len()),
            // The manager fills owner registration (it owns the
            // inbound-owner registry); the bridge cannot observe it.
            inbound_owner_registered: false,
            inbound_owner_count: 0,
        })
    }

    /// Plan 212 §6 — returns the installed inbound receive ids, if
    /// any. Used by production provisioning to register
    /// `register_inbound_tunnel_owner` for every live receive id.
    pub(crate) fn router_inbound_receive_ids(&self) -> Vec<u32> {
        self.router_network
            .as_ref()
            .map(|state| state.inbound_receive_ids.clone())
            .unwrap_or_default()
    }

    /// Removes the activated outbound role belonging to one evicted
    /// canonical group pool slot. Dropping the role releases its secrets.
    pub(crate) fn remove_router_outbound_pool_slot(&mut self, pool_slot: TunnelSlot) -> bool {
        let Some(state) = self.router_network.as_mut() else {
            return false;
        };
        let before = state.outbound_roles.len();
        state
            .outbound_roles
            .retain(|binding| binding.pool_slot != pool_slot);
        if state.outbound_roles.is_empty() {
            state.next_outbound_role = 0;
        } else {
            state.next_outbound_role %= state.outbound_roles.len();
        }
        state.outbound_expires_at_ms = state
            .outbound_roles
            .iter()
            .map(|binding| binding.role.expires_at_ms())
            .max()
            .unwrap_or(0);
        state.outbound_roles.len() != before
    }

    /// Removes the router receive id for an evicted group inbound pool
    /// entry. Owner-table removal is performed by `ServiceTunnelManager`.
    pub(crate) fn remove_router_inbound_receive(&mut self, receive_id: u32) -> bool {
        let Some(state) = self.router_network.as_mut() else {
            return false;
        };
        let before = state.inbound_receive_ids.len();
        state.inbound_receive_ids.retain(|id| *id != receive_id);
        state.inbound_receive_ids.len() != before
    }

    /// Adds an outbound data-plane projection for a newly registered pool
    /// slot. Duplicate slots fail closed.
    pub(crate) fn append_router_outbound_role(
        &mut self,
        pool_slot: TunnelSlot,
        role: DestinationOutboundRole,
    ) -> Result<(), String> {
        let state = self
            .router_network
            .as_mut()
            .ok_or_else(|| "router-backed network state not installed".to_owned())?;
        if state
            .outbound_roles
            .iter()
            .any(|binding| binding.pool_slot == pool_slot)
        {
            return Err("outbound pool slot already has a data-plane role".to_owned());
        }
        state.outbound_expires_at_ms = state.outbound_expires_at_ms.max(role.expires_at_ms());
        state
            .outbound_roles
            .push(GroupOutboundRole { pool_slot, role });
        Ok(())
    }

    /// Adds an inbound receive id after its established material has entered
    /// the canonical pool and inbound dispatch registry.
    pub(crate) fn append_router_inbound_receive(
        &mut self,
        receive_id: u32,
    ) -> Result<(), InboundReceiveProjectionError> {
        let state = self
            .router_network
            .as_mut()
            .ok_or(InboundReceiveProjectionError::RouterStateMissing)?;
        if state.inbound_receive_ids.contains(&receive_id) {
            return Err(InboundReceiveProjectionError::DuplicateReceiveId);
        }
        state.inbound_receive_ids.push(receive_id);
        Ok(())
    }

    /// Replaces the signed group LeaseSet and current receive-owner projection
    /// after the canonical pool advances. A below-minimum pool is made
    /// immediately unusable by setting its inbound deadline to `now_ms`.
    pub(crate) fn refresh_router_pool_snapshot(
        &mut self,
        lease_set2: Option<(LeaseSet2, ValidatedLeaseSet2)>,
        inbound_receive_ids: Vec<u32>,
        inbound_expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), String> {
        let state = self
            .router_network
            .as_mut()
            .ok_or_else(|| "router-backed network state not installed".to_owned())?;
        if let Some((lease_set2, validated)) = lease_set2 {
            let now_seconds = u32::try_from(now_ms / 1000).unwrap_or(u32::MAX);
            ValidatedLeaseSet2::from_lease_set2(
                lease_set2.clone(),
                Some(self.identity.id().as_netdb_key()),
                LeaseSet2ValidationContext::new(now_seconds),
            )
            .map_err(|error| format!("refreshed router LS2 validation failed: {error:?}"))?;
            state.lease_set2 = Some(lease_set2);
            state.validated_lease_set2 = Some(validated);
            state.inbound_expires_at_ms = inbound_expires_at_ms;
        } else {
            state.inbound_expires_at_ms = now_ms;
        }
        state.inbound_receive_ids = inbound_receive_ids;
        Ok(())
    }

    /// Plan 212 §13 — clones the router-backed signed LS2 for
    /// server-side publication. Returns `None` when no
    /// router-backed state is installed. The LS2 is public
    /// material (destination + leases + signature); no secret
    /// material crosses this boundary.
    pub(crate) fn router_ls2_for_publication(&self) -> Option<LeaseSet2> {
        self.router_network
            .as_ref()
            .and_then(|state| state.lease_set2.clone())
    }

    /// Plan 212 §12 — borrows the router-backed outbound role for
    /// NetDB lookup/publication composition. Returns `None` when
    /// no router-backed state is installed. The borrow never moves
    /// the role; installation ownership stays with the service
    /// runtime.
    pub(crate) fn router_outbound_role_ref(
        &mut self,
        now_ms: u64,
    ) -> Option<&DestinationOutboundRole> {
        let state = self.router_network.as_mut()?;
        let count = state.outbound_roles.len();
        for offset in 0..count {
            let index = (state.next_outbound_role + offset) % count;
            if state.outbound_roles[index].role.expires_at_ms() > now_ms {
                state.next_outbound_role = (index + 1) % count;
                return state.outbound_roles.get(index).map(|binding| &binding.role);
            }
        }
        None
    }

    /// Plan 212 §11 — installs a validated remote LeaseSet2 into
    /// the router-backed routing state (never the local fabric
    /// routing). Fails when router-backed state is missing or
    /// expired.
    pub(crate) fn install_remote_lease_set2_into_router_state(
        &mut self,
        validated: ValidatedLeaseSet2,
        now_ms: u64,
    ) -> Result<i2pr_netdb::DestinationHash, String> {
        let state = self
            .router_network
            .as_mut()
            .ok_or_else(|| "router-backed network state not installed (NotInstalled)".to_owned())?;
        if state.is_expired(now_ms) {
            return Err("router-backed network state expired (NoTunnelMaterial)".to_owned());
        }
        // Plan 213 corrective: mirror the validated reference LS2
        // into the bridge-level sender-resolution directory. The
        // router-state routing store drives outbound composition,
        // but `dispatch_router_garlic_to_canonical_streaming` resolves
        // the reply sender through `self.lease_set2_store`; without
        // the mirror a decryptable SYN-ACK fails closed with
        // `UnknownDestination` and the stream never establishes.
        let _ = self.lease_set2_store.insert(validated.clone());
        state
            .routing
            .install_remote_lease_set2(validated)
            .map_err(|error| error.to_string())
    }

    /// Plan 213 — returns the validated remote LeaseSet2 the
    /// authenticated inbound path mirrored into the bridge-level
    /// sender-resolution directory (`install_remote_lease_set2_into_router_state`),
    /// keyed by NetDB destination hash. The production remote route
    /// consults this mirror when the backend coordinator cache
    /// misses so a just-authenticated inbound handshake can answer
    /// without a second lookup round-trip. Returns `None` when no
    /// validated record exists for the hash (fail-closed).
    pub(crate) fn cached_router_remote_lease_set2(
        &self,
        key: &i2pr_netdb::DestinationHash,
    ) -> Option<ValidatedLeaseSet2> {
        self.lease_set2_store.get(key).cloned()
    }

    /// Plan 212 §11 — runs the canonical
    /// [`StreamingDestinationAdapter::send`] explicitly against
    /// router-backed state (never the local fabric fields).
    ///
    /// Fails closed with a typed string when router-backed state is
    /// missing or expired. The queued `TransportSendRequest` comes
    /// from the canonical service `StreamingManager`; the same
    /// Destination identity signs the send.
    pub(crate) fn compose_router_send(
        &mut self,
        request: &TransportSendRequest,
        now_seconds: u32,
        now_ms: u64,
    ) -> Result<i2pr_client::OutboundDeliveryPlan, String> {
        let state = self
            .router_network
            .as_mut()
            .ok_or_else(|| "router-backed network state not installed (NotInstalled)".to_owned())?;
        if state.is_expired(now_ms) {
            return Err("router-backed network state expired (NoTunnelMaterial)".to_owned());
        }
        let local_id = self.identity.id();
        debug_assert_eq!(state.destination_id, local_id);
        let local_static_secret = *self.identity.static_secret_bytes();
        let Some(local_lease_set2) = state.lease_set2.clone() else {
            return Err("router-backed LeaseSet2 is not ready (NoTunnelMaterial)".to_owned());
        };
        let mut os_rng = OsRng;
        let mut rng = UnwrapMut(&mut os_rng);
        let routing = &state.routing;
        let session = &mut state.session_manager;
        let count = state.outbound_roles.len();
        if count == 0 {
            return Err("router-backed outbound pool is empty (NoTunnelMaterial)".to_owned());
        }
        let selected = (0..count).find_map(|offset| {
            let index = (state.next_outbound_role + offset) % count;
            (state.outbound_roles[index].role.expires_at_ms() > now_ms).then_some(index)
        });
        let Some(selected) = selected else {
            return Err("router-backed outbound pool is expired (NoTunnelMaterial)".to_owned());
        };
        state.next_outbound_role = (selected + 1) % count;
        let outbound = &state.outbound_roles[selected].role;
        StreamingDestinationAdapter::send(
            request,
            routing,
            session,
            outbound,
            local_id,
            &local_static_secret,
            &local_lease_set2,
            now_seconds,
            now_ms,
            &mut rng,
        )
        .map_err(|error| error.to_string())
    }
}

/// Plan 486 — which `StreamingManager` owns one dequeued inbound
/// payload. The canonical manager owns outbound SYN state; the
/// receiver mirror owns the inbound listener surface that STREAM
/// ACCEPT and FORWARD drain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InboundStreamingTarget {
    Canonical,
    Mirror,
}

/// Plan 486 — how one dequeued payload settled, so the drain loop
/// can attribute it without re-matching the adapter outcome.
enum DequeuedPayloadOutcome {
    StreamingAccepted,
    StreamingRejected,
    DatagramAccepted,
}

/// Plan 486 — waiter-scoped inbound-SYN classification. Server
/// profiles always feed the receiver mirror (Plan 213, unchanged).
/// Client profiles feed the mirror only when a STREAM ACCEPT waiter
/// is parked for the destination AND the payload has the inbound-SYN
/// shape (SYNCHRONIZE set, originator stream ids); every other
/// packet — SYN-ACKs, DATA, datagrams, undecodable bytes — keeps
/// profile routing, so the working forward leg never moves. Any
/// classification failure routes by profile (fail-closed to the
/// pre-486 behavior).
pub(crate) fn classify_inbound_streaming_target(
    payload_bytes: &[u8],
    is_server: bool,
    mirror_accept_parked: bool,
) -> InboundStreamingTarget {
    if is_server {
        return InboundStreamingTarget::Mirror;
    }
    if !mirror_accept_parked {
        return InboundStreamingTarget::Canonical;
    }
    if matches!(
        inbound_payload_shape(payload_bytes),
        InboundPayloadShape::OriginatorSyn
    ) {
        InboundStreamingTarget::Mirror
    } else {
        InboundStreamingTarget::Canonical
    }
}

/// Plan 487 WP3 — the bounded window after the streaming connection
/// reaches the established state before the live stall attribution is
/// emitted. The window must be long enough for a healthy data phase
/// to deliver, and short enough that the receipt is still inside the
/// driver's read timeout.
pub(crate) const DATA_PHASE_STALL_WINDOW_MS: u64 = 20_000;

/// Plan 487 WP3 — the live DATA-phase stall snapshot: the four
/// stages, each read from its own counter, plus the per-manager
/// ingress split that separates "the DATA never arrived" from "the
/// DATA arrived in the wrong manager".
///
/// Counts only; no identities, keys, ports, stream ids, or payload
/// material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DataPhaseStallSnapshot {
    pub(crate) now_ms: u64,
    /// Application payload bytes actually handed to the application.
    pub(crate) app_bytes_drained: u64,
    pub(crate) mirror_observed: u64,
    pub(crate) mirror_accepted: u64,
    pub(crate) mirror_rejected: u64,
    pub(crate) mirror_payload_bytes: u64,
    pub(crate) mirror_arrived: u64,
    pub(crate) canonical_observed: u64,
    pub(crate) canonical_accepted: u64,
    pub(crate) canonical_rejected: u64,
    pub(crate) canonical_payload_bytes: u64,
    pub(crate) canonical_arrived: u64,
    pub(crate) mirror_ack_generated: u64,
    pub(crate) mirror_ack_transmitted: u64,
    pub(crate) mirror_ack_no_ack_emitted: u64,
    pub(crate) canonical_ack_generated: u64,
    pub(crate) canonical_ack_transmitted: u64,
    pub(crate) ack_egress_sent: u64,
    pub(crate) ack_egress_dropped: u64,
}

/// Plan 487 WP2 — logs one four-stage DATA-phase attribution receipt.
///
/// The local-only (SAM-side) and the router-network (service-side)
/// driver each own a destination's streaming state in their own mode,
/// so the receipt is written once here and called from both. A
/// receipt that only one mode can produce is a receipt that is dead
/// in the other mode, and a dead attribution cannot classify a stall.
///
/// `terminal` selects the wording of the live (window-based) receipt
/// against the terminal (end-of-data-phase) receipt. The field set is
/// identical: every stage is read from the counter that owns it, and
/// no stage is inferred from another stage's count.
///
/// Counts only; no identities, keys, ports, stream ids, or payload
/// material.
pub(crate) fn log_data_phase_stall_attribution(snapshot: &DataPhaseStallSnapshot, terminal: bool) {
    let message = if terminal {
        "STREAM ACCEPT data phase ended with no DATA at the owning manager; terminal four-stage attribution (counts only)"
    } else {
        "STREAM ACCEPT data phase stalled with no DATA at the owning manager; live four-stage attribution (counts only)"
    };
    tracing::warn!(
        app_bytes_drained = snapshot.app_bytes_drained,
        mirror_observed = snapshot.mirror_observed,
        mirror_accepted = snapshot.mirror_accepted,
        mirror_rejected = snapshot.mirror_rejected,
        mirror_payload_bytes = snapshot.mirror_payload_bytes,
        mirror_arrived = snapshot.mirror_arrived,
        canonical_observed = snapshot.canonical_observed,
        canonical_accepted = snapshot.canonical_accepted,
        canonical_rejected = snapshot.canonical_rejected,
        canonical_payload_bytes = snapshot.canonical_payload_bytes,
        canonical_arrived = snapshot.canonical_arrived,
        mirror_ack_generated = snapshot.mirror_ack_generated,
        mirror_ack_transmitted = snapshot.mirror_ack_transmitted,
        mirror_ack_no_ack_emitted = snapshot.mirror_ack_no_ack_emitted,
        canonical_ack_generated = snapshot.canonical_ack_generated,
        canonical_ack_transmitted = snapshot.canonical_ack_transmitted,
        ack_egress_sent = snapshot.ack_egress_sent,
        ack_egress_dropped = snapshot.ack_egress_dropped,
        transition = "stream_accept_data_phase_stalled",
        "{message}",
    );
}

/// Plan 487 WP3 — the shape of one already garlic-authenticated
/// inbound payload, as a single bounded peek.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InboundPayloadShape {
    /// An originator SYN: SYNCHRONIZE set, `send_stream_id == 0`,
    /// `receive_stream_id != 0`.
    OriginatorSyn,
    /// An established-phase packet (DATA, ACK, CLOSE, RESET, or a
    /// SYN-ACK): its `send_stream_id` is the **local** stream id
    /// the peer uses to address us.
    Established {
        /// The local receive stream id the peer addresses us by.
        local_receive_stream_id: u32,
    },
    /// Not a streaming packet, or not decodable.
    Other,
}

/// Plan 487 WP3 — bounded shape peek over an already garlic
/// authenticated inner I2NP short-transport message. Requires the
/// I2NP Data body and the I2CP streaming-protocol envelope, and
/// applies the manager's own originator-SYN rule (SYNCHRONIZE set,
/// `send_stream_id == 0`, `receive_stream_id != 0`) through the same
/// header peek the manager routes on. Pure bounded decodes over
/// authenticated bytes: the garlic/auth path is untouched and the
/// adapter still performs its own full decode downstream. Any failure
/// reports `Other` so the caller keeps profile routing.
///
/// The established-phase branch reports the packet's **`send_stream_id`**
/// as the local receive stream id. That is the id the peer uses to
/// address us: a peer sends `sendStreamId = <our receive id>` and
/// `receiveStreamId = <the peer's own id>`, which is exactly how the
/// manager resolves an established-phase packet — it looks the
/// connection up in `inbound_by_stream` / `outbound_by_stream`, and
/// both tables are keyed by `local_receive_stream_id`. Reading the
/// `receive_stream_id` field instead would report the *peer's* id,
/// which is never a key in either table, so every owner lookup would
/// miss and every established-phase packet would fall back to profile
/// routing.
fn inbound_payload_shape(payload_bytes: &[u8]) -> InboundPayloadShape {
    let Ok(message) = I2npMessage::decode_short_transport(payload_bytes, MAX_I2NP_PAYLOAD_SIZE)
    else {
        return InboundPayloadShape::Other;
    };
    let I2npBody::Data(body) = message.body() else {
        return InboundPayloadShape::Other;
    };
    let Ok(envelope) = decode_i2cp_data_body(body.payload.as_bytes()) else {
        return InboundPayloadShape::Other;
    };
    if envelope.protocol != PROTOCOL_TYPE_STREAMING {
        return InboundPayloadShape::Other;
    }
    let Ok(peek) = peek_streaming_header(&envelope.payload) else {
        return InboundPayloadShape::Other;
    };
    if peek.flags_bits & FLAG_SYNCHRONIZE != 0
        && peek.send_stream_id == 0
        && peek.receive_stream_id != 0
    {
        InboundPayloadShape::OriginatorSyn
    } else {
        InboundPayloadShape::Established {
            local_receive_stream_id: peek.send_stream_id,
        }
    }
}

impl SamDestinationBridge {
    /// Plan 486 — routes one already-dequeued inbound payload to
    /// its owning `StreamingManager` (waiter-scoped per
    /// [`classify_inbound_streaming_target`]) and feeds it through
    /// [`StreamingDestinationAdapter::receive`]. Extracted verbatim
    /// from the drain loop so the routing decision is unit-testable
    /// without ECIES/garlic material; the loop only attributes the
    /// returned outcome.
    /// Plan 487 WP3 — the manager that owns one local receive stream
    /// id.
    ///
    /// An accepted (inbound) connection lives in the receiver mirror;
    /// an opened (outbound) connection lives in the canonical manager.
    /// The lookup is by the **local receive stream id** — the id the
    /// peer uses to address us — and never by the waiter's state.
    fn established_phase_owner(
        &self,
        local_receive_stream_id: u32,
    ) -> Option<InboundStreamingTarget> {
        if self
            .receiver_streaming()
            .lookup_inbound(local_receive_stream_id)
            .is_some()
        {
            return Some(InboundStreamingTarget::Mirror);
        }
        if self
            .streaming()
            .lookup_outbound(local_receive_stream_id)
            .is_some()
        {
            return Some(InboundStreamingTarget::Canonical);
        }
        None
    }

    /// Plan 487 WP3 — resolves the manager that owns one inbound
    /// payload.
    ///
    /// An established-phase packet (DATA, ACK, CLOSE, RESET, SYN-ACK)
    /// belongs to a connection that already exists, so it is routed to
    /// that connection's owner.
    ///
    /// Plan 486's waiter-scoped rule moved only the inbound SYN to the
    /// mirror and kept profile routing for every other packet. That
    /// mis-routed the inbound DATA of an accepted connection to the
    /// canonical manager, where no application reads it: the payload
    /// was never delivered, the connection was never acknowledged, and
    /// the peer eventually abandoned it. The owner is resolved from the
    /// packet's own stream ids, so the routing no longer depends on
    /// whether a STREAM ACCEPT waiter is still parked.
    fn classify_inbound_target(
        &self,
        payload_bytes: &[u8],
        is_server: bool,
        mirror_accept_parked: bool,
    ) -> InboundStreamingTarget {
        if let InboundPayloadShape::Established {
            local_receive_stream_id,
        } = inbound_payload_shape(payload_bytes)
            && let Some(owner) = self.established_phase_owner(local_receive_stream_id)
        {
            return owner;
        }
        classify_inbound_streaming_target(payload_bytes, is_server, mirror_accept_parked)
    }

    fn receive_dequeued_payload(
        &mut self,
        payload_bytes: &[u8],
        remote_hash: &[u8; 32],
        now_ms: u64,
        is_server: bool,
        mirror_accept_parked: bool,
    ) -> DequeuedPayloadOutcome {
        let target = self.classify_inbound_target(payload_bytes, is_server, mirror_accept_parked);
        let receive_outcome = match target {
            InboundStreamingTarget::Mirror => StreamingDestinationAdapter::receive(
                payload_bytes,
                &self.identity,
                &mut self.receiver_streaming,
                remote_hash,
                now_ms,
            ),
            InboundStreamingTarget::Canonical => StreamingDestinationAdapter::receive(
                payload_bytes,
                &self.identity,
                &mut self.streaming,
                remote_hash,
                now_ms,
            ),
        };
        match receive_outcome {
            Ok(i2pr_client::streaming_adapter::InboundStreamingOutcome::StreamingDispatched {
                ..
            }) => DequeuedPayloadOutcome::StreamingAccepted,
            Ok(i2pr_client::streaming_adapter::InboundStreamingOutcome::UnsupportedProtocol {
                protocol: _,
            }) => DequeuedPayloadOutcome::StreamingRejected,
            Ok(i2pr_client::streaming_adapter::InboundStreamingOutcome::DatagramReceived {
                protocol,
                source_port,
                destination_port,
                payload,
            }) => {
                // Plan 291: repliable datagrams authenticate
                // here; raw datagrams bound-check here. Either
                // rejection counts against the datagram path,
                // never the streaming path.
                if self
                    .datagrams
                    .process_inbound(
                        protocol,
                        source_port,
                        destination_port,
                        &payload,
                        *remote_hash,
                        now_ms,
                    )
                    .is_ok()
                {
                    DequeuedPayloadOutcome::DatagramAccepted
                } else {
                    DequeuedPayloadOutcome::StreamingRejected
                }
            }
            Err(_) => DequeuedPayloadOutcome::StreamingRejected,
        }
    }

    /// Plan 212 §14 — decodes one recovered standard I2NP Garlic
    /// envelope against router-backed ECIES/session/routing state,
    /// drains every authenticated destination payload via
    /// `DestinationDispatcher::pop_payload(local_destination)`, and
    /// delivers each payload into the owning service
    /// `StreamingManager` used by the application socket pump via
    /// `StreamingDestinationAdapter::receive`.
    ///
    /// Required sequence:
    /// 1. decode recovered standard Garlic envelope;
    /// 2. `dispatch_garlic_envelope` against router-backed state;
    /// 3. on reject -> typed rejected report;
    /// 4. repeatedly `pop_payload(local_destination_id)`;
    /// 5. for every dequeued Data payload:
    ///    `StreamingDestinationAdapter::receive(bytes, identity,
    ///    target_streaming, remote_destination_hash, now_ms)`, where
    ///    the target is the canonical service `StreamingManager` for
    ///    client profiles and the receiver mirror for server profiles
    ///    (Plan 213: the polled server loop accepts from the mirror
    ///    through the same tested path as local traffic; Plan 486:
    ///    a parked STREAM ACCEPT waiter additionally routes inbound
    ///    SYNs to the mirror);
    /// 6. normal SYN/SYN-ACK/DATA/ACK/close processing preserved;
    /// 7. caller wakes the existing delivery driver when receive
    ///    queued an outbound response (see
    ///    `response_queued_for_delivery`);
    /// 8. caller advances `remote_inbound_dispatched` ONLY after
    ///    `streaming_packets_accepted >= 1` (enforced by the
    ///    manager/service-product seam, not here).
    ///
    /// The remote peer hash is never the router hash: for a new
    /// inbound session it derives from the validated remote LS2 key
    /// returned by the dispatcher; for subsequent packets it uses
    /// the peer Destination hash associated with the established
    /// session (`sender_destination` when present, else the
    /// validated LS2 key installed during the handshake).
    pub(crate) fn dispatch_router_garlic_to_canonical_streaming(
        &mut self,
        bytes: &[u8],
        now_seconds: u32,
        now_ms: u64,
        is_server: bool,
        mirror_accept_parked: bool,
    ) -> Result<RouterInboundDispatchReport, String> {
        let state = self
            .router_network
            .as_mut()
            .ok_or_else(|| "router-backed network state not installed (NotInstalled)".to_owned())?;
        if state.is_expired(now_ms) {
            return Err("router-backed network state expired (NoTunnelMaterial)".to_owned());
        }
        let envelope =
            i2pr_proto::I2npMessage::decode_standard(bytes, i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
                .map_err(|error| format!("decode_standard failed: {error:?}"))?;
        if !matches!(envelope.body(), i2pr_proto::I2npBody::Garlic(_)) {
            return Err("envelope is not a standard I2NP Garlic body".to_owned());
        }
        let local_id = self.identity.id();
        let local_static_secret = *self.identity.static_secret_bytes();
        let local_static_public = self.identity.static_public_bytes();
        // Router-backed dispatcher/routing/session/store live in the
        // installed state, except the dispatcher itself which stays
        // on the bridge (canonical destination dispatcher). The
        // session + routing + store used here are the router-backed
        // ones so local-fabric material can never authenticate
        // remote traffic.
        //
        // Note: the bridge's canonical `dispatcher` is the delivery
        // queue owner; the router-backed `routing`/`session_manager`
        // supply the cryptographic/lease state. This keeps one
        // canonical queue while separating key material.
        let outcome = self.dispatcher.dispatch_garlic_envelope(
            &mut state.session_manager,
            local_id,
            &local_static_secret,
            &local_static_public,
            now_seconds,
            &envelope,
            &mut self.lease_set2_store,
        );
        // Derive the authenticated remote Destination hash. Never
        // substitute the router hash.
        let remote_hash_opt: Option<[u8; 32]> = match &outcome {
            i2pr_client::InboundDispatchOutcome::NewSessionProcessed {
                remote_destination_hash,
                validated_remote_lease_set2,
                ..
            } => {
                let _ = state
                    .routing
                    .install_remote_lease_set2((**validated_remote_lease_set2).clone());
                Some(*remote_destination_hash.as_bytes())
            }
            i2pr_client::InboundDispatchOutcome::ExistingSessionProcessed {
                sender_destination,
                remote_static_public,
                ..
            } => {
                if let Some(hash) = sender_destination {
                    Some(*hash.as_bytes())
                } else {
                    // Fall back to the routing table's validated
                    // knowledge keyed by static public: search the
                    // router-backed routing cache for a record whose
                    // usable X25519 key matches. When absent, the
                    // payload cannot be attributed to a remote
                    // Destination and the report records zero
                    // accepted (fail-closed, no global fallback).
                    let _ = remote_static_public;
                    None
                }
            }
            i2pr_client::InboundDispatchOutcome::NewSessionReplyProcessed {
                sender_destination,
                ..
            } => sender_destination.as_ref().map(|hash| *hash.as_bytes()),
            i2pr_client::InboundDispatchOutcome::Rejected(error) => {
                self.diagnostics.record_inbound_observation();
                let report = RouterInboundDispatchReport {
                    garlic_authenticated: false,
                    garlic_rejection: Some(classify_router_garlic_rejection(error)),
                    payloads_dequeued: 0,
                    streaming_packets_accepted: 0,
                    streaming_rejected: 0,
                    datagrams_accepted: 0,
                    synack_observed: 0,
                    synack_accepted: 0,
                };
                self.diagnostics.record_dispatch_report(&report);
                return Ok(report);
            }
        };
        let Some(remote_hash) = remote_hash_opt else {
            self.diagnostics.record_inbound_observation();
            let report = RouterInboundDispatchReport {
                garlic_authenticated: true,
                garlic_rejection: None,
                payloads_dequeued: 0,
                streaming_packets_accepted: 0,
                streaming_rejected: 0,
                datagrams_accepted: 0,
                synack_observed: 0,
                synack_accepted: 0,
            };
            self.diagnostics.record_dispatch_report(&report);
            return Ok(report);
        };
        // Plan 478 handshake attribution: snapshot the target
        // manager's SYN-response accounting around the payload drain
        // so arrival (`observed`) and acceptance (`accepted`) are
        // distinguished by source-owned counts.
        let synack_before = if is_server {
            self.receiver_streaming.syn_response_ingress()
        } else {
            self.streaming.syn_response_ingress()
        };
        // Drain every queued payload for the local destination (not
        // just the first clove) into the SAME canonical service
        // `StreamingManager` the application pump reads.
        let mut dequeued = 0_usize;
        let mut accepted = 0_usize;
        let mut rejected = 0_usize;
        let mut datagrams_accepted = 0_usize;
        while let Some(payload) = self.dispatcher.pop_payload(local_id) {
            dequeued = dequeued.saturating_add(1);
            match self.receive_dequeued_payload(
                payload.bytes(),
                &remote_hash,
                now_ms,
                is_server,
                mirror_accept_parked,
            ) {
                DequeuedPayloadOutcome::StreamingAccepted => {
                    accepted = accepted.saturating_add(1);
                }
                DequeuedPayloadOutcome::StreamingRejected => {
                    rejected = rejected.saturating_add(1);
                }
                DequeuedPayloadOutcome::DatagramAccepted => {
                    datagrams_accepted = datagrams_accepted.saturating_add(1);
                }
            }
        }
        if accepted > 0 || datagrams_accepted > 0 {
            self.diagnostics.record_inbound_dispatch();
        } else {
            self.diagnostics.record_inbound_observation();
        }
        let synack_after = if is_server {
            self.receiver_streaming.syn_response_ingress()
        } else {
            self.streaming.syn_response_ingress()
        };
        let synack_delta = synack_after.delta_since(synack_before);
        let report = RouterInboundDispatchReport {
            garlic_authenticated: true,
            garlic_rejection: None,
            payloads_dequeued: dequeued,
            streaming_packets_accepted: accepted,
            streaming_rejected: rejected,
            datagrams_accepted,
            synack_observed: usize::try_from(synack_delta.observed).unwrap_or(usize::MAX),
            synack_accepted: usize::try_from(synack_delta.accepted).unwrap_or(usize::MAX),
        };
        self.diagnostics.record_dispatch_report(&report);
        Ok(report)
    }
}

fn classify_router_garlic_rejection(
    error: &i2pr_client::InboundDispatchError,
) -> RouterGarlicRejection {
    use i2pr_client::InboundDispatchError as Error;
    match error {
        Error::NotGarlic
        | Error::Codec(_)
        | Error::EnvelopeTooShort { .. }
        | Error::UnsupportedGarlicFlag(_) => RouterGarlicRejection::Structural,
        Error::Session(i2pr_client::EciesSessionError::UnknownSessionTag)
        | Error::Session(i2pr_client::EciesSessionError::NoPendingHandshake)
        | Error::Session(i2pr_client::EciesSessionError::NoProvisionalResponder)
        | Error::Session(i2pr_client::EciesSessionError::NoSession) => {
            RouterGarlicRejection::SessionTag
        }
        Error::Session(i2pr_client::EciesSessionError::Ecies(
            i2pr_crypto::EciesError::AuthenticationFailed,
        )) => RouterGarlicRejection::EciesAuthentication,
        Error::Session(i2pr_client::EciesSessionError::Ecies(_)) => {
            RouterGarlicRejection::EciesOther
        }
        Error::Payload(_) | Error::QueueFull(_) => RouterGarlicRejection::Payload,
        Error::MissingSenderLeaseSet2 => RouterGarlicRejection::MissingSenderLeaseSet2,
        Error::LeaseSet2Validation(i2pr_netdb::LeaseSet2ValidationError::InvalidSignature) => {
            RouterGarlicRejection::LeaseSet2Signature
        }
        Error::LeaseSet2Validation(
            i2pr_netdb::LeaseSet2ValidationError::Expired { .. }
            | i2pr_netdb::LeaseSet2ValidationError::AllLeasesExpired
            | i2pr_netdb::LeaseSet2ValidationError::ExcessiveFuture { .. }
            | i2pr_netdb::LeaseSet2ValidationError::OfflineSignatureExpired,
        ) => RouterGarlicRejection::LeaseSet2Freshness,
        Error::LeaseSet2Validation(i2pr_netdb::LeaseSet2ValidationError::DestinationMismatch) => {
            RouterGarlicRejection::LeaseSet2Destination
        }
        Error::LeaseSet2Validation(
            i2pr_netdb::LeaseSet2ValidationError::NoUsableX25519
            | i2pr_netdb::LeaseSet2ValidationError::DuplicateX25519,
        ) => RouterGarlicRejection::LeaseSet2KeyPolicy,
        Error::LeaseSet2Validation(
            i2pr_netdb::LeaseSet2ValidationError::BlindedPublicationDeferred,
        ) => RouterGarlicRejection::LeaseSet2BlindedPolicy,
        Error::LeaseSet2Validation(i2pr_netdb::LeaseSet2ValidationError::Crypto(
            i2pr_crypto::CryptoError::Protocol(_),
        )) => RouterGarlicRejection::LeaseSet2CryptoProtocol,
        Error::LeaseSet2Validation(i2pr_netdb::LeaseSet2ValidationError::Crypto(
            i2pr_crypto::CryptoError::UnsupportedAlgorithm {
                algorithm,
                context: "signature verification",
            },
        )) => RouterGarlicRejection::LeaseSet2UnsupportedSignatureType(*algorithm),
        Error::LeaseSet2Validation(i2pr_netdb::LeaseSet2ValidationError::Crypto(
            i2pr_crypto::CryptoError::InvalidKey { .. }
            | i2pr_crypto::CryptoError::AllZeroSharedSecret,
        )) => RouterGarlicRejection::LeaseSet2CryptoKey,
        Error::LeaseSet2Validation(i2pr_netdb::LeaseSet2ValidationError::Crypto(_)) => {
            RouterGarlicRejection::LeaseSet2Crypto
        }
        Error::LeaseSet2Validation(
            i2pr_netdb::LeaseSet2ValidationError::EncodedTooLarge { .. }
            | i2pr_netdb::LeaseSet2ValidationError::ArithmeticOverflow,
        ) => RouterGarlicRejection::LeaseSet2Size,
        Error::SenderKeyMismatch => RouterGarlicRejection::SenderKeyMismatch,
        Error::UnknownDestination(_) => RouterGarlicRejection::UnknownDestination,
        Error::Session(_) | Error::DuplicateReplay => RouterGarlicRejection::Other,
    }
}

/// Per-destination handle shared by every per-stream socket task.
#[derive(Clone)]
pub struct SamDestinationHandle {
    inner: Arc<Mutex<SamDestinationBridge>>,
}

impl std::fmt::Debug for SamDestinationHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SamDestinationHandle")
            .finish_non_exhaustive()
    }
}

impl SamDestinationHandle {
    pub fn new(bridge: SamDestinationBridge) -> Self {
        Self {
            inner: Arc::new(Mutex::new(bridge)),
        }
    }

    pub fn with<R>(&self, closure: impl FnOnce(&mut SamDestinationBridge) -> R) -> R {
        let mut guard = self.inner.lock().expect("sam bridge mutex poisoned");
        closure(&mut guard)
    }

    pub fn into_inner(self) -> Arc<Mutex<SamDestinationBridge>> {
        self.inner
    }

    pub fn inner(&self) -> &Arc<Mutex<SamDestinationBridge>> {
        &self.inner
    }

    /// Installs the per-destination inbound-tunnel factory used by
    /// the Plan 147 runtime driver.
    pub fn install_inbound_tunnel_factory(
        &self,
        factory: Arc<dyn InboundTunnelFactory>,
    ) -> Option<Arc<dyn InboundTunnelFactory>> {
        self.with(|bridge| bridge.install_inbound_tunnel_factory(factory))
    }

    /// Returns a clone of the installed inbound-tunnel factory, if
    /// any.
    pub fn inbound_tunnel_factory(&self) -> Option<Arc<dyn InboundTunnelFactory>> {
        self.with(|bridge| bridge.inbound_tunnel_factory())
    }
}

/// Bounded per-destination bridge registry.
#[derive(Default)]
pub struct SamDestinations {
    by_id: HashMap<DestinationId, SamDestinationHandle>,
    /// Plan 144: peer-destination-hash -> local-destination-id reverse
    /// index. Lets the per-stream raw byte bridge look up the peer
    /// bridge from a SAM `TransportSendRequest.destination_hash`
    /// without scanning every bridge.
    by_peer: HashMap<[u8; 32], DestinationId>,
}

impl std::fmt::Debug for SamDestinations {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SamDestinations")
            .field("count", &self.by_id.len())
            .finish()
    }
}

impl SamDestinations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn install(
        &mut self,
        destination_id: DestinationId,
        bridge: SamDestinationBridge,
    ) -> SamDestinationHandle {
        let peer_hash = bridge.identity_destination_hash();
        let handle = SamDestinationHandle::new(bridge);
        // Plan 144: the per-stream raw byte bridge uses the
        // `destination_hash` carried on every `TransportSendRequest`
        // to route outbound traffic to the correct peer. Map both keys
        // so a single `install` call wires the reverse index.
        if let Some(prior) = self.by_id.insert(destination_id, handle.clone()) {
            // Drop the prior reverse-index entry (different local
            // destination, same peer hash) so the index never
            // references a removed bridge.
            let _ = self.by_peer.remove(&prior.peer_destination_hash());
        }
        self.by_peer.insert(peer_hash, destination_id);
        handle
    }

    /// Plan 180 §3 install path: accepts a pre-built
    /// [`SamDestinationHandle`] instead of consuming the bridge by
    /// value. Used when the same bridge handle is shared with the
    /// staged `ServiceRuntime` and the manager-level mirror.
    pub fn install_handle(
        &mut self,
        destination_id: DestinationId,
        handle: SamDestinationHandle,
    ) -> SamDestinationHandle {
        let peer_hash = handle.peer_destination_hash();
        if let Some(prior) = self.by_id.insert(destination_id, handle.clone()) {
            let _ = self.by_peer.remove(&prior.peer_destination_hash());
        }
        self.by_peer.insert(peer_hash, destination_id);
        handle
    }

    pub fn get(&self, destination_id: DestinationId) -> Option<SamDestinationHandle> {
        self.by_id.get(&destination_id).cloned()
    }

    pub fn debug_ids(&self) -> Vec<DestinationId> {
        self.by_id.keys().copied().collect()
    }

    pub fn remove(&mut self, destination_id: DestinationId) -> Option<SamDestinationHandle> {
        let removed = self.by_id.remove(&destination_id);
        if let Some(handle) = &removed {
            self.by_peer.remove(&handle.peer_destination_hash());
        }
        removed
    }

    /// Returns the bridge registered for the supplied peer
    /// destination hash (the SAM `PUB` value the destination owns).
    pub fn lookup_by_peer_hash(&self, peer_hash: &[u8; 32]) -> Option<SamDestinationHandle> {
        let local_id = self.by_peer.get(peer_hash).copied()?;
        let handle = self.by_id.get(&local_id).cloned()?;
        handle
            .with(|bridge| bridge.has_local_product_material())
            .then_some(handle)
    }

    /// Resolves a peer destination hash to the locally-owned bridge's
    /// signed LeaseSet2 and NetDB key, validating the record through
    /// the canonical [`i2pr_netdb::ValidatedLeaseSet2`] gate before
    /// returning it. Plan 149 §7 forbids a real external client from
    /// installing peer LeaseSet2 routing manually; the SAM service
    /// owns the local directory and only hands out records it has
    /// validated itself.
    pub fn resolve_local_lease_set2(
        &self,
        peer_hash: &[u8; 32],
        now_seconds: u32,
    ) -> Result<
        Option<(i2pr_netdb::ValidatedLeaseSet2, DestinationId)>,
        i2pr_netdb::LeaseSet2ValidationError,
    > {
        let Some(local_id) = self.by_peer.get(peer_hash).copied() else {
            return Ok(None);
        };
        let Some(handle) = self.by_id.get(&local_id).cloned() else {
            return Ok(None);
        };
        let Some((lease_set2, identity_key)) = handle.with(|bridge| {
            if !bridge.has_local_product_material() {
                return None;
            }
            Some((bridge.lease_set2()?.clone(), bridge.identity_netdb_key()))
        }) else {
            return Ok(None);
        };
        let validated = i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
            lease_set2,
            Some(identity_key),
            i2pr_netdb::LeaseSet2ValidationContext::new(now_seconds),
        )?;
        Ok(Some((validated, local_id)))
    }
}

impl SamDestinationHandle {
    /// Returns the peer destination hash that the handle's bridge
    /// advertises in its LeaseSet2 (used by the per-stream raw byte
    /// bridge to clean up the peer reverse-index on `SamDestinations::remove`).
    pub fn peer_destination_hash(&self) -> [u8; 32] {
        self.with(|bridge| bridge.identity_destination_hash())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SamBridgeBuildError {
    #[error("signed LeaseSet2 construction failed: {0}")]
    LeaseSet(LeaseSetError),
    #[error("destination identity construction failed: {0}")]
    Identity(#[from] i2pr_client::DestinationIdentityError),
    #[error("destination pool rejected: {0}")]
    Pool(#[from] i2pr_client::DestinationPoolError),
    #[error("destination pool produced no inbound lease sources")]
    EmptyPool,
}

impl From<SamBridgeBuildError> for SamServiceError {
    fn from(error: SamBridgeBuildError) -> Self {
        SamServiceError::InvalidConfig(format!("sam bridge build failed: {error}"))
    }
}

impl From<LeaseSetError> for SamBridgeBuildError {
    fn from(error: LeaseSetError) -> Self {
        Self::LeaseSet(error)
    }
}

/// Builds a SAM bridge from real established outbound material and
/// a signed LeaseSet2. The inbound tunnel is held by the daemon's
/// `SamServiceState::streaming_pools` outside the bridge so the
/// bridge can drive multiple deliveries per destination without
/// `Clone` on `EstablishedTunnel`.
pub fn build_sam_destination_bridge(
    identity: DestinationIdentity,
    lease_set2: LeaseSet2,
    outbound_role: DestinationOutboundRole,
    now_seconds: u32,
) -> Result<SamDestinationBridge, SamBridgeBuildError> {
    Ok(SamDestinationBridge::new(
        identity,
        lease_set2,
        outbound_role,
        now_seconds,
    ))
}

#[derive(Debug)]
pub enum BridgeDeliveryError {
    UnknownPeer([u8; 32]),
    Delivery(LocalDeliveryError),
    NotStreaming,
    MissingLocalFabricMaterial,
}

impl std::fmt::Display for BridgeDeliveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPeer(hash) => write!(formatter, "no peer bridge registered for {hash:?}"),
            Self::Delivery(error) => write!(formatter, "local delivery failed: {error}"),
            Self::NotStreaming => formatter.write_str("inbound observation not protocol 6"),
            Self::MissingLocalFabricMaterial => {
                formatter.write_str("local fabric material is unavailable for this bridge")
            }
        }
    }
}

impl std::error::Error for BridgeDeliveryError {}

impl From<LocalDeliveryError> for BridgeDeliveryError {
    fn from(error: LocalDeliveryError) -> Self {
        Self::Delivery(error)
    }
}

/// Checked-out sender/receiver delivery state for one bridge-to-peer
/// delivery. The bridge fields are not `mut` at the struct level, so
/// deliveries swap them with empty placeholders, run, then move them
/// back. Shared by the single and batched seams so both traverse the
/// identical swap discipline.
struct CheckedOutDelivery {
    sender_identity_arc: Arc<DestinationIdentity>,
    sender_outbound_role: DestinationOutboundRole,
    sender_lease_set2: LeaseSet2,
    sender_routing: DestinationRouting,
    sender_session: EciesSessionManager,
    receiver_dispatcher: DestinationDispatcher,
    receiver_session: EciesSessionManager,
    receiver_routing: DestinationRouting,
    receiver_streaming: StreamingManager,
    receiver_datagrams: i2pr_client::datagram::DatagramManager,
    peer_canonical_streaming: StreamingManager,
    receiver_lease_set2_store: LeaseSet2Store,
    receiver_now_seconds: u32,
    identity_arc: Arc<DestinationIdentity>,
    inbound_hop1_hash: Hash,
    inbound_hop2_hash: Hash,
}

/// Swaps the delivery fields out of both bridges and extracts the
/// inbound tunnel's hop hashes without holding either bridge lock.
fn checkout_delivery(
    sender: &SamDestinationHandle,
    peer: &SamDestinationHandle,
    peer_inbound_tunnel: &EstablishedTunnel,
) -> Result<CheckedOutDelivery, BridgeDeliveryError> {
    if !sender.with(|bridge| bridge.has_local_product_material())
        || !peer.with(|bridge| bridge.has_local_product_material())
    {
        return Err(BridgeDeliveryError::MissingLocalFabricMaterial);
    }
    // Step 1: extract the hop hashes from the inbound tunnel so the
    // delivery can use them without holding the bridge lock.
    let inbound_hop1_hash = peer_inbound_tunnel
        .hops()
        .first()
        .map_or(Hash::from_bytes([0_u8; 32]), |hop| hop.peer().hash());
    let inbound_hop2_hash = peer_inbound_tunnel
        .hops()
        .get(1)
        .map_or(Hash::from_bytes([0_u8; 32]), |hop| hop.peer().hash());

    // Step 2: take the peer receiver-state fields out of the peer
    // bridge (and the sender fields out of the sender bridge). The
    // bridge fields are not `mut` at the struct level, so we swap
    // them with empty placeholders, run the delivery, then swap them
    // back.
    let receiver_dispatcher = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(&mut peer_guard.receiver_dispatcher, empty_dispatcher())
    };
    let receiver_session = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.receiver_session,
            EciesSessionManager::new(EciesSessionConfig::balanced()),
        )
    };
    let receiver_routing = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.receiver_routing,
            DestinationRouting::new(DestinationRoutingConfig::balanced()),
        )
    };
    let receiver_streaming = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.receiver_streaming,
            StreamingManager::new(StreamingConfig::balanced()),
        )
    };
    let receiver_datagrams = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.datagrams,
            i2pr_client::datagram::DatagramManager::new(),
        )
    };
    let peer_canonical_streaming = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.streaming,
            StreamingManager::new(StreamingConfig::balanced()),
        )
    };
    let receiver_lease_set2_store = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::take(&mut peer_guard.receiver_lease_set2_store)
    };
    let receiver_now_seconds = {
        let peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        peer_guard.receiver_now_seconds
    };
    let identity_arc = {
        let peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        Arc::clone(&peer_guard.identity)
    };
    let sender_identity_arc = {
        let sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        Arc::clone(&sender_guard.identity)
    };
    let sender_outbound_role = {
        let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        sender_guard
            .outbound_role
            .take()
            .ok_or(BridgeDeliveryError::MissingLocalFabricMaterial)?
    };
    let sender_lease_set2 = {
        let sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        sender_guard
            .lease_set2
            .clone()
            .ok_or(BridgeDeliveryError::MissingLocalFabricMaterial)?
    };
    let sender_routing = {
        let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        std::mem::replace(
            &mut sender_guard.routing,
            DestinationRouting::new(DestinationRoutingConfig::balanced()),
        )
    };
    let sender_session = {
        let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        std::mem::replace(
            &mut sender_guard.session_manager,
            EciesSessionManager::new(EciesSessionConfig::balanced()),
        )
    };
    // Return every field by value; the caller bundles them into the
    // delivery inputs it needs.
    Ok(CheckedOutDelivery {
        sender_identity_arc,
        sender_outbound_role,
        sender_lease_set2,
        sender_routing,
        sender_session,
        receiver_dispatcher,
        receiver_session,
        receiver_routing,
        receiver_streaming,
        receiver_datagrams,
        peer_canonical_streaming,
        receiver_lease_set2_store,
        receiver_now_seconds,
        identity_arc,
        inbound_hop1_hash,
        inbound_hop2_hash,
    })
}

/// Moves checked-out delivery fields back into their owning bridges.
///
/// Plan 149 §7: the receiver routing was extracted from the peer's
/// CANONICAL `routing` field, so the modified routing (with the
/// freshly installed remote LeaseSet2) must land back in the
/// canonical field. The original `receiver_routing` mirror field
/// is untouched by this call and stays where it was.
fn restore_delivery(
    sender: &SamDestinationHandle,
    peer: &SamDestinationHandle,
    checked: CheckedOutDelivery,
) {
    let CheckedOutDelivery {
        sender_routing,
        sender_session,
        sender_outbound_role,
        receiver_dispatcher,
        receiver_session,
        receiver_routing,
        receiver_streaming,
        receiver_datagrams,
        peer_canonical_streaming,
        receiver_lease_set2_store,
        ..
    } = checked;
    {
        let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        sender_guard.routing = sender_routing;
        sender_guard.session_manager = sender_session;
        sender_guard.outbound_role = Some(sender_outbound_role);
    }
    {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        peer_guard.record_inbound_dispatch();
        peer_guard.receiver_dispatcher = receiver_dispatcher;
        peer_guard.receiver_session = receiver_session;
        peer_guard.routing = receiver_routing;
        peer_guard.receiver_streaming = receiver_streaming;
        peer_guard.datagrams = receiver_datagrams;
        peer_guard.streaming = peer_canonical_streaming;
        peer_guard.receiver_lease_set2_store = receiver_lease_set2_store;
    }
}

/// Drives one outbound `TransportSendRequest` from one bridge
/// into the peer bridge's receiver mirror using the full Plan 129
/// stack. The peer inbound tunnel is supplied by the caller (the
/// daemon's `SamServiceState::streaming_pools`) because
/// `EstablishedTunnel` does not implement `Clone` and the seam
/// consumes it once per delivery.
#[allow(clippy::too_many_arguments)]
pub fn bridge_to_peer<R: CryptoRng + RngCore>(
    sender: &SamDestinationHandle,
    peer: &SamDestinationHandle,
    outbound_hop0_hash: Hash,
    outbound_hop1_hash: Hash,
    request: &TransportSendRequest,
    now_seconds: u32,
    now_ms: u64,
    outbound_tunnel_id: TunnelId,
    peer_inbound_tunnel: EstablishedTunnel,
    rng: &mut R,
) -> Result<(), BridgeDeliveryError> {
    let mut checked = checkout_delivery(sender, peer, &peer_inbound_tunnel)?;
    let mut sender_inputs = LocalDeliverySender {
        identity: &checked.sender_identity_arc,
        routing: &mut checked.sender_routing,
        session: &mut checked.sender_session,
        outbound: &checked.sender_outbound_role,
        local_lease_set2: &checked.sender_lease_set2,
        now_seconds,
        now_ms,
    };
    let mut receiver_inputs = LocalDeliveryReceiver {
        identity: &checked.identity_arc,
        dispatcher: &mut checked.receiver_dispatcher,
        session: &mut checked.receiver_session,
        routing: &mut checked.receiver_routing,
        streaming: &mut checked.receiver_streaming,
        datagrams: &mut checked.receiver_datagrams,
        canonical_streaming: Some(&mut checked.peer_canonical_streaming),
        lease_set2_store: &mut checked.receiver_lease_set2_store,
        now_seconds: checked.receiver_now_seconds,
    };

    let outcome = deliver(
        request,
        &mut sender_inputs,
        &mut receiver_inputs,
        outbound_hop0_hash,
        outbound_hop1_hash,
        peer_inbound_tunnel,
        checked.inbound_hop1_hash,
        checked.inbound_hop2_hash,
        outbound_tunnel_id,
        rng,
    );

    {
        let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        sender_guard.record_outbound_dispatch(request.clone());
    }
    restore_delivery(sender, peer, checked);
    {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        peer_guard.record_inbound_dispatch();
    }

    let outcome = outcome?;
    match outcome {
        LocalDeliveryOutcome::Delivered { observation } => {
            let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
            peer_guard.record_inbound_observation();
            drop(peer_guard);
            if matches!(
                observation,
                i2pr_client::streaming_adapter::InboundStreamingOutcome::StreamingDispatched { .. }
            ) {
                Ok(())
            } else {
                Err(BridgeDeliveryError::NotStreaming)
            }
        }
        LocalDeliveryOutcome::DatagramDelivered => {
            let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
            peer_guard.record_inbound_observation();
            drop(peer_guard);
            Ok(())
        }
        LocalDeliveryOutcome::DispatchRejected(_) => Ok(()),
    }
}

/// Drives a same-remote batch of `TransportSendRequest`s from one
/// bridge into the peer bridge's receiver mirror as one bundled New
/// Session Reply when the destination's reply-bundling policy enables
/// it (Plan 296).
///
/// The swap discipline, carrier contract, and diagnostics recording
/// match [`bridge_to_peer`]; only payload assembly differs (one
/// multi-clove reply instead of one message per request). The caller
/// groups consecutive same-remote requests; mixed-remote input
/// returns [`BatchedAttempt::Singles`] and the caller runs the
/// single seam per request. A bundled-but-undeliverable batch
/// reports every index failed without retry: the session already
/// advanced, so re-sending would duplicate application bytes.
#[allow(clippy::too_many_arguments)]
pub fn bridge_to_peer_batched<R: CryptoRng + RngCore>(
    sender: &SamDestinationHandle,
    peer: &SamDestinationHandle,
    outbound_hop0_hash: Hash,
    outbound_hop1_hash: Hash,
    requests: &[TransportSendRequest],
    bundling: ReplyBundling,
    now_seconds: u32,
    now_ms: u64,
    outbound_tunnel_id: TunnelId,
    peer_inbound_tunnel: EstablishedTunnel,
    rng: &mut R,
) -> BatchedAttempt {
    let mut checked = match checkout_delivery(sender, peer, &peer_inbound_tunnel) {
        Ok(checked) => checked,
        Err(_) => {
            return BatchedAttempt::Singles {
                decode_failed: Vec::new(),
            };
        }
    };
    let mut sender_inputs = LocalDeliverySender {
        identity: &checked.sender_identity_arc,
        routing: &mut checked.sender_routing,
        session: &mut checked.sender_session,
        outbound: &checked.sender_outbound_role,
        local_lease_set2: &checked.sender_lease_set2,
        now_seconds,
        now_ms,
    };
    let mut receiver_inputs = LocalDeliveryReceiver {
        identity: &checked.identity_arc,
        dispatcher: &mut checked.receiver_dispatcher,
        session: &mut checked.receiver_session,
        routing: &mut checked.receiver_routing,
        streaming: &mut checked.receiver_streaming,
        datagrams: &mut checked.receiver_datagrams,
        canonical_streaming: Some(&mut checked.peer_canonical_streaming),
        lease_set2_store: &mut checked.receiver_lease_set2_store,
        now_seconds: checked.receiver_now_seconds,
    };
    let attempt = deliver_batched(
        requests,
        bundling,
        &mut sender_inputs,
        &mut receiver_inputs,
        outbound_hop0_hash,
        outbound_hop1_hash,
        peer_inbound_tunnel,
        checked.inbound_hop1_hash,
        checked.inbound_hop2_hash,
        outbound_tunnel_id,
        rng,
    );
    match attempt {
        BatchedAttempt::Bundled(report) => {
            {
                let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
                for index in &report.delivered {
                    sender_guard.record_outbound_dispatch(requests[*index].clone());
                }
            }
            restore_delivery(sender, peer, checked);
            {
                let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
                for _ in &report.delivered {
                    peer_guard.record_inbound_dispatch();
                }
                for _ in &report.observed {
                    peer_guard.record_inbound_observation();
                }
            }
            BatchedAttempt::Bundled(report)
        }
        BatchedAttempt::Singles { decode_failed } => {
            restore_delivery(sender, peer, checked);
            BatchedAttempt::Singles { decode_failed }
        }
    }
}

fn empty_dispatcher() -> DestinationDispatcher {
    DestinationDispatcher::new()
}

/// Factory trait that constructs a fresh inbound `EstablishedTunnel`
/// for the bridge's destination. The runtime driver calls this
/// once per outbound `TransportSendRequest`; each call returns a
/// new tunnel because the Plan 129 local seam consumes its
/// argument. Production deployments bind a real inbound-tunnel
/// pool here; tests bind the deterministic fixture
/// `established_inbound(seed)` builder.
pub trait InboundTunnelFactory: Send + Sync {
    /// Builds a fresh inbound `EstablishedTunnel` for the
    /// destination this factory is registered against.
    fn build_inbound_tunnel(&self) -> Result<EstablishedTunnel, InboundTunnelBuildError>;
}

/// Typed failure of [`InboundTunnelFactory::build_inbound_tunnel`].
#[derive(Debug, thiserror::Error)]
pub enum InboundTunnelBuildError {
    /// The factory could not produce a tunnel (e.g., pool empty).
    #[error("inbound tunnel pool exhausted")]
    PoolExhausted,
    /// Underlying tunnel material was malformed.
    #[error("inbound tunnel material invalid: {0}")]
    InvalidMaterial(String),
}

/// Decodes a SAM public destination Base64 text into a
/// `(DestinationId, SigningPublicKey, StaticPublicKey)` triple.
/// Retained for backwards compatibility with the Plan 138
/// test seam. Only X25519/type-4 Destinations carry the static key
/// directly; ElGamal/type-0 Destinations (Plan 223) return
/// `StaticPublicKeyLength` so the caller resolves the active X25519 key
/// via the local LeaseSet2 directory instead of the Destination field.
pub fn decode_destination_triple(
    text: &str,
) -> Result<
    (
        DestinationId,
        i2pr_proto::SigningPublicKey,
        [u8; i2pr_crypto::X25519_KEY_LENGTH],
    ),
    SamDestinationTripleError,
> {
    use i2pr_api::sam::base64;
    let bytes = base64::decode(text, i2pr_api::sam::private_destination::PUB_LENGTH)
        .map_err(|_| SamDestinationTripleError::Base64)?;
    let destination =
        i2pr_proto::Destination::decode(&bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(SamDestinationTripleError::Destination)?;
    let hash = destination
        .hash()
        .map_err(SamDestinationTripleError::Destination)?;
    let id = DestinationId::from_hash(hash);
    let signing_key = destination.signing_key().clone();
    let mut static_public = [0_u8; i2pr_crypto::X25519_KEY_LENGTH];
    let pk_bytes = destination.public_key().as_bytes();
    if pk_bytes.len() != i2pr_crypto::X25519_KEY_LENGTH {
        return Err(SamDestinationTripleError::StaticPublicKeyLength);
    }
    static_public.copy_from_slice(&pk_bytes[..i2pr_crypto::X25519_KEY_LENGTH]);
    Ok((id, signing_key, static_public))
}

/// Decodes a SAM public destination Base64 text into its
/// `(DestinationId, SigningPublicKey)` pair without requiring the
/// Destination encryption field to carry the X25519 static key.
///
/// Plan 223: router-owned Destinations use ElGamal/type-0 legacy identity
/// material (256-byte filler) while the active X25519 key lives in
/// Standard LS2. STREAM CONNECT resolves that key via the local LS2
/// directory; this helper supplies the id + signing key for both shapes.
pub fn decode_destination_id_and_signing(
    text: &str,
) -> Result<(DestinationId, i2pr_proto::SigningPublicKey), SamDestinationTripleError> {
    use i2pr_api::sam::base64;
    let bytes = base64::decode(text, i2pr_api::sam::private_destination::PUB_LENGTH)
        .map_err(|_| SamDestinationTripleError::Base64)?;
    let destination =
        i2pr_proto::Destination::decode(&bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(SamDestinationTripleError::Destination)?;
    let hash = destination
        .hash()
        .map_err(SamDestinationTripleError::Destination)?;
    let id = DestinationId::from_hash(hash);
    let signing_key = destination.signing_key().clone();
    Ok((id, signing_key))
}

#[derive(Debug, thiserror::Error)]
pub enum SamDestinationTripleError {
    #[error("sam base64 decode failed")]
    Base64,
    #[error("destination decode failed: {0}")]
    Destination(i2pr_proto::CodecError),
    #[error("destination encryption key length mismatch")]
    StaticPublicKeyLength,
}

#[cfg(test)]
mod router_network_bridge_tests {
    use super::*;

    #[test]
    fn router_network_bridge_starts_without_local_fabric_material() {
        let mut rng = OsRng;
        let identity = Arc::new(DestinationIdentity::generate(&mut rng).expect("identity"));
        let destination_id = identity.id();
        let peer_hash = *destination_id.as_hash().as_bytes();
        let bridge = SamDestinationBridge::with_router_identity(
            identity,
            1_700_000_000,
            StreamingConfig::balanced(),
        );
        assert!(bridge.lease_set2().is_none());
        assert!(bridge.outbound_role().is_none());
        assert!(bridge.inbound_tunnel_factory().is_none());
        assert!(!bridge.has_router_network_state(0));

        let mut destinations = SamDestinations::new();
        destinations.install(destination_id, bridge);
        assert!(destinations.lookup_by_peer_hash(&peer_hash).is_none());
        assert!(
            destinations
                .resolve_local_lease_set2(&peer_hash, 1_700_000_000)
                .expect("no local LeaseSet2 validation attempted")
                .is_none()
        );
    }

    #[test]
    fn dispatch_report_outcomes_accumulate_without_touching_legacy_counts() {
        let mut diagnostics = BridgeDiagnostics::new();
        // Garlic-rejected envelope: only the rejected outcome moves.
        diagnostics.record_dispatch_report(&RouterInboundDispatchReport {
            garlic_authenticated: false,
            garlic_rejection: Some(RouterGarlicRejection::EciesAuthentication),
            payloads_dequeued: 0,
            streaming_packets_accepted: 0,
            streaming_rejected: 0,
            datagrams_accepted: 0,
            synack_observed: 0,
            synack_accepted: 0,
        });
        // Authenticated but unattributed: counts as authenticated with
        // zero dequeued, so "arrived but never dequeued" stays visible.
        diagnostics.record_dispatch_report(&RouterInboundDispatchReport {
            garlic_authenticated: true,
            garlic_rejection: None,
            payloads_dequeued: 0,
            streaming_packets_accepted: 0,
            streaming_rejected: 0,
            datagrams_accepted: 0,
            synack_observed: 0,
            synack_accepted: 0,
        });
        // Authenticated with a two-payload drain, one accepted.
        diagnostics.record_dispatch_report(&RouterInboundDispatchReport {
            garlic_authenticated: true,
            garlic_rejection: None,
            payloads_dequeued: 2,
            streaming_packets_accepted: 1,
            streaming_rejected: 1,
            datagrams_accepted: 0,
            synack_observed: 0,
            synack_accepted: 0,
        });
        assert_eq!(diagnostics.garlic_authenticated(), 2);
        assert_eq!(diagnostics.garlic_rejected(), 1);
        assert_eq!(diagnostics.payloads_dequeued(), 2);
        assert_eq!(diagnostics.streaming_accepted(), 1);
        // The legacy Plan 210 counters are owned by their own call
        // sites; outcome folding never moves them.
        assert_eq!(diagnostics.inbound_dispatched(), 0);
        assert_eq!(diagnostics.inbound_observations(), 0);
    }

    use i2pr_client::streaming::connection::ConnectionId;
    use i2pr_client::streaming::manager::{
        ConnectOutcome, DEFAULT_ADVERTISED_MAX_PAYLOAD, RemoteDestination,
    };
    use i2pr_proto::streaming::decode_client_payload;
    use i2pr_proto::{DeferredPayload, OpaqueMessageBody, encode_i2cp_data_body};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    const PLAN486_NOW_MS: u64 = 1_700_000_000_000;

    /// Builds a router-identity bridge pair (no LeaseSet2, no
    /// outbound role — the inbound SYN path needs neither).
    fn plan486_bridge() -> (Arc<DestinationIdentity>, SamDestinationBridge) {
        let mut rng = ChaCha8Rng::seed_from_u64(0x4860_0001);
        let identity = Arc::new(DestinationIdentity::generate(&mut rng).expect("identity"));
        let bridge = SamDestinationBridge::with_router_identity(
            Arc::clone(&identity),
            1_700_000_000,
            StreamingConfig::balanced(),
        );
        (identity, bridge)
    }

    fn plan486_remote(identity: &DestinationIdentity) -> RemoteDestination {
        RemoteDestination {
            destination_hash: *identity.id().as_hash().as_bytes(),
            signing_public_key: identity.destination().signing_key().clone(),
            static_public_key: identity.static_public_bytes(),
        }
    }

    /// Builds the exact inner-I2NP bytes the router seam dequeues
    /// for one sender-produced Streaming packet: the sender
    /// manager's gzip client payload is unwrapped (as the ECIES /
    /// garlic layers would deliver it post-authentication),
    /// re-wrapped in the I2CP Data body with the envelope's wire
    /// ports, and framed as a short-transport I2NP Data message.
    /// Garlic/ECIES authentication is out of scope here by design:
    /// these tests prove the waiter-scoped manager selection, not
    /// the crypto that precedes it.
    fn plan486_inner_i2np_bytes(application_payload: &[u8]) -> Vec<u8> {
        let envelope = decode_client_payload(
            application_payload,
            i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES,
        )
        .expect("client-payload envelope");
        let body_bytes = encode_i2cp_data_body(
            envelope.source_port,
            envelope.destination_port,
            envelope.protocol,
            &envelope.payload,
        )
        .expect("I2CP data body");
        let message = I2npMessage::new_short_transport(
            0x4860_00b5,
            1_700_000_000,
            I2npBody::Data(OpaqueMessageBody {
                payload: DeferredPayload::new(body_bytes, MAX_I2NP_PAYLOAD_SIZE)
                    .expect("bounded body"),
            }),
        )
        .expect("short-transport message");
        message
            .encode_short_transport_to_vec(MAX_I2NP_PAYLOAD_SIZE)
            .expect("short-transport encoding")
    }

    /// Produces one real signed originator SYN (wire destination
    /// port 0, the lane shape) and returns its inner-I2NP bytes.
    fn plan486_stock_shaped_syn(
        sender: &mut StreamingManager,
        originator: &DestinationIdentity,
        recipient_remote: &RemoteDestination,
        seed: u64,
    ) -> Vec<u8> {
        let ConnectOutcome::SynSent { .. } = sender
            .connect(
                originator,
                recipient_remote,
                0,
                0,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                PLAN486_NOW_MS,
                &mut ChaCha8Rng::seed_from_u64(seed),
            )
            .expect("connect")
        else {
            panic!("expected outbound SYN");
        };
        let request = sender.drain_outbound().pop().expect("SYN request");
        plan486_inner_i2np_bytes(&request.application_payload)
    }

    #[test]
    fn parked_client_syn_reaches_mirror_backlog() {
        // Plan 486 WP1 lane shape: GenericClient-profiled bridge
        // (`is_server = false`), mirror wildcard bound exactly as
        // `execute_stream_accept` binds it, stock-shaped inbound SYN
        // (wire destination port 0), ACCEPT waiter parked.
        let (recipient, mut bridge) = plan486_bridge();
        bridge
            .receiver_streaming_mut()
            .listen(0)
            .expect("mirror wildcard listen");
        let mut rng = ChaCha8Rng::seed_from_u64(0x4860_0002);
        let originator = DestinationIdentity::generate(&mut rng).expect("originator");
        let mut sender = StreamingManager::new(StreamingConfig::balanced());
        let recipient_remote = plan486_remote(&recipient);
        let syn =
            plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4860_0003);
        let originator_hash = *originator.id().as_hash().as_bytes();
        let outcome =
            bridge.receive_dequeued_payload(&syn, &originator_hash, PLAN486_NOW_MS, false, true);
        assert!(
            matches!(outcome, DequeuedPayloadOutcome::StreamingAccepted),
            "parked SYN must dispatch to the mirror, not reject"
        );
        // Mirror side: accepted into the backlog the ACCEPT waiter
        // drains; canonical side: untouched (no no_listener).
        assert_eq!(bridge.receiver_streaming().listener_backlog(0), 1);
        assert_eq!(bridge.receiver_streaming().syn_ingress().accepted, 1);
        assert_eq!(bridge.receiver_streaming().syn_ingress().observed, 1);
        let canonical = bridge.streaming().syn_ingress();
        assert_eq!(canonical.observed, 0);
        assert_eq!(canonical.rejected, 0);
        assert_eq!(canonical.rejected_no_listener, 0);
    }

    #[test]
    fn unparked_client_syn_stays_canonical_no_listener() {
        // Same lane shape with no waiter parked: the SYN keeps
        // profile routing and fails closed on the canonical manager
        // (the pre-486 behavior, locked as the diagnosis exhibit).
        let (recipient, mut bridge) = plan486_bridge();
        bridge
            .receiver_streaming_mut()
            .listen(0)
            .expect("mirror wildcard listen");
        let mut rng = ChaCha8Rng::seed_from_u64(0x4860_0004);
        let originator = DestinationIdentity::generate(&mut rng).expect("originator");
        let mut sender = StreamingManager::new(StreamingConfig::balanced());
        let recipient_remote = plan486_remote(&recipient);
        let syn =
            plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4860_0005);
        let originator_hash = *originator.id().as_hash().as_bytes();
        let outcome =
            bridge.receive_dequeued_payload(&syn, &originator_hash, PLAN486_NOW_MS, false, false);
        assert!(
            matches!(outcome, DequeuedPayloadOutcome::StreamingRejected),
            "unparked SYN must keep canonical profile routing"
        );
        let canonical = bridge.streaming().syn_ingress();
        assert_eq!(canonical.observed, 1);
        assert_eq!(canonical.rejected, 1);
        assert_eq!(canonical.rejected_no_listener, 1);
        assert_eq!(bridge.receiver_streaming().listener_backlog(0), 0);
        assert_eq!(bridge.receiver_streaming().syn_ingress().observed, 0);
    }

    #[test]
    fn parked_client_synack_stays_canonical() {
        // Plan 486 forward-guard: with an ACCEPT waiter parked, only
        // inbound SYNs move. A SYN-ACK answering the canonical
        // outbound CONNECT keeps profile routing so the working
        // forward leg never migrates.
        let (recipient, mut bridge) = plan486_bridge();
        bridge
            .receiver_streaming_mut()
            .listen(0)
            .expect("mirror wildcard listen");
        let mut rng = ChaCha8Rng::seed_from_u64(0x4860_0006);
        let originator = DestinationIdentity::generate(&mut rng).expect("originator");
        let originator_hash = *originator.id().as_hash().as_bytes();
        // Canonical outbound CONNECT toward the originator.
        let ConnectOutcome::SynSent { .. } = bridge
            .streaming_mut()
            .connect(
                &recipient,
                &plan486_remote(&originator),
                0,
                0,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                PLAN486_NOW_MS,
                &mut ChaCha8Rng::seed_from_u64(0x4860_0007),
            )
            .expect("bridge canonical connect")
        else {
            panic!("expected outbound SYN");
        };
        let connect_request = bridge.streaming_mut().drain_outbound().pop().expect("SYN");
        let connect_envelope = decode_client_payload(
            &connect_request.application_payload,
            i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES,
        )
        .expect("SYN envelope");
        // The originator answers with a SYN-ACK for the canonical
        // outbound stream ids.
        let mut originator_manager = StreamingManager::new(StreamingConfig::balanced());
        originator_manager.listen(0).expect("originator listen");
        originator_manager
            .process_inbound_packet(
                &connect_envelope.payload,
                recipient.id().as_hash().as_bytes(),
                &originator,
                0,
                0,
                PLAN486_NOW_MS,
            )
            .expect("originator processes SYN");
        let originator_cid = originator_manager
            .accept(0, PLAN486_NOW_MS)
            .expect("originator backlog");
        let syn_response = originator_manager
            .accept_inbound_syn(
                &originator,
                &plan486_remote(&recipient),
                originator_cid,
                0,
                0,
                DEFAULT_ADVERTISED_MAX_PAYLOAD,
                PLAN486_NOW_MS,
                &mut ChaCha8Rng::seed_from_u64(0x4860_0008),
            )
            .expect("originator SYN response");
        let synack_bytes = plan486_inner_i2np_bytes(&syn_response.application_payload);
        // The SYN-ACK arrives while still parked: it must stay
        // canonical (the outbound CONNECT lives there).
        let outcome = bridge.receive_dequeued_payload(
            &synack_bytes,
            &originator_hash,
            PLAN486_NOW_MS,
            false,
            true,
        );
        assert!(
            matches!(outcome, DequeuedPayloadOutcome::StreamingAccepted),
            "parked SYN-ACK must stay canonical"
        );
        assert_eq!(bridge.streaming().syn_response_ingress().accepted, 1);
        assert_eq!(
            bridge.receiver_streaming().syn_response_ingress().observed,
            0
        );
        assert_eq!(bridge.receiver_streaming().syn_ingress().observed, 0);
    }

    #[test]
    fn server_profile_ignores_parked_flag() {
        // Server profiles feed the mirror with or without a waiter
        // (Plan 213, unchanged by the waiter-scoped rule).
        let (recipient, mut bridge) = plan486_bridge();
        bridge
            .receiver_streaming_mut()
            .listen(0)
            .expect("mirror wildcard listen");
        let mut rng = ChaCha8Rng::seed_from_u64(0x4860_000b);
        let originator = DestinationIdentity::generate(&mut rng).expect("originator");
        let mut sender = StreamingManager::new(StreamingConfig::balanced());
        let recipient_remote = plan486_remote(&recipient);
        let syn =
            plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4860_000c);
        let originator_hash = *originator.id().as_hash().as_bytes();
        for parked in [false, true] {
            let outcome = bridge.receive_dequeued_payload(
                &syn,
                &originator_hash,
                PLAN486_NOW_MS,
                true,
                parked,
            );
            assert!(
                matches!(outcome, DequeuedPayloadOutcome::StreamingAccepted),
                "server SYN must reach the mirror (parked={parked})"
            );
        }
        assert_eq!(bridge.receiver_streaming().syn_ingress().accepted, 2);
        assert_eq!(bridge.streaming().syn_ingress().observed, 0);
    }

    #[test]
    fn corrupt_envelope_fails_closed_to_profile_routing() {
        // Undecodable bytes can never be classified as a SYN: they
        // keep profile routing and reject on the canonical manager.
        let (_recipient, mut bridge) = plan486_bridge();
        bridge
            .receiver_streaming_mut()
            .listen(0)
            .expect("mirror wildcard listen");
        let garbage = vec![0xFF_u8; 64];
        let outcome =
            bridge.receive_dequeued_payload(&garbage, &[0x11_u8; 32], PLAN486_NOW_MS, false, true);
        assert!(matches!(outcome, DequeuedPayloadOutcome::StreamingRejected));
        assert_eq!(bridge.receiver_streaming().syn_ingress().observed, 0);
        // The canonical manager peeked a non-SYN (garbage fails the
        // header peek before any SYN accounting starts).
        assert_eq!(bridge.streaming().syn_ingress().observed, 0);
    }

    /// Plan 487 WP3 — builds the inner-I2NP bytes a peer would send
    /// for one established-phase DATA packet on `local_receive_stream_id`.
    fn plan487_inner_i2np_data_bytes(
        local_receive_stream_id: u32,
        remote_stream_id: u32,
        payload: &[u8],
    ) -> Vec<u8> {
        // The peer addresses us with `sendStreamId = <our receive id>`
        // and `receiveStreamId = <the peer's own id>`. This is the
        // same field assignment the manager's established-phase arm
        // resolves on, so the packet must be built exactly that way.
        let builder = i2pr_proto::streaming::StreamingPacketBuilder {
            send_stream_id: local_receive_stream_id,
            receive_stream_id: remote_stream_id,
            sequence_num: 1,
            ack_through: 0,
            nacks: Vec::new(),
            resend_delay: 0,
            flags: i2pr_proto::streaming::StreamingFlags::empty(),
            option_bytes: Vec::new(),
            payload: payload.to_vec(),
        };
        let wire = i2pr_proto::streaming::encode_streaming_packet(
            &builder,
            i2pr_proto::streaming::StreamingSendLimit::default(),
        )
        .expect("streaming data packet");
        let envelope =
            i2pr_proto::streaming::encode_client_payload(&i2pr_proto::streaming::ClientPayload {
                protocol: i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER,
                source_port: 0,
                destination_port: 0,
                payload: wire,
            })
            .expect("client payload");
        plan486_inner_i2np_bytes(&envelope)
    }

    #[test]
    fn established_phase_data_reaches_the_manager_that_owns_the_connection() {
        // Plan 487 WP3: an accepted (inbound) connection lives in the
        // receiver mirror. Its DATA must be routed there by the
        // connection's owner, **not** by whether a STREAM ACCEPT
        // waiter is still parked: the waiter is satisfied by the
        // handshake, so a waiter-scoped rule mis-routed every
        // subsequent DATA of an accepted connection to the canonical
        // manager, where no application reads it.
        let (recipient, mut bridge) = plan486_bridge();
        bridge
            .receiver_streaming_mut()
            .listen(0)
            .expect("mirror wildcard listen");
        let mut rng = ChaCha8Rng::seed_from_u64(0x4870_0001);
        let originator = DestinationIdentity::generate(&mut rng).expect("originator");
        let mut sender = StreamingManager::new(StreamingConfig::balanced());
        let recipient_remote = plan486_remote(&recipient);
        // Burn one originator stream id first. Both the originator's
        // SYN and the mirror's inbound connection allocate from the
        // same `0x8000_0000` seed, so without this the two ids would
        // coincide and a swapped field assignment would pass by
        // accident. A real peer's id is independent of ours.
        let _ = plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4870_0009);
        let syn =
            plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4870_0002);
        // The waiter is parked: the SYN reaches the mirror.
        let outcome =
            bridge.receive_dequeued_payload(&syn, &[0x22_u8; 32], PLAN486_NOW_MS, false, true);
        assert!(matches!(outcome, DequeuedPayloadOutcome::StreamingAccepted));
        // The application accepts the connection; the waiter is now
        // satisfied and no longer parked.
        let connection_id = bridge
            .receiver_streaming_mut()
            .accept(0, PLAN486_NOW_MS)
            .expect("accepted connection");
        let local_receive_stream_id = bridge
            .receiver_streaming()
            .get_connection(connection_id)
            .expect("connection")
            .local_stream_id();
        let remote_stream_id = bridge
            .receiver_streaming()
            .get_connection(connection_id)
            .expect("connection")
            .remote_stream_id();
        assert_ne!(
            local_receive_stream_id, remote_stream_id,
            "the local and peer stream ids must differ, otherwise this test \
             cannot tell the two wire fields apart"
        );
        assert!(
            bridge
                .receiver_streaming()
                .lookup_inbound(local_receive_stream_id)
                .is_some()
        );

        // The peer's DATA, with no waiter parked.
        let data =
            plan487_inner_i2np_data_bytes(local_receive_stream_id, remote_stream_id, b"payload");
        let before_mirror = bridge.receiver_streaming().data_packet_ingress();
        let before_canonical = bridge.streaming().data_packet_ingress();
        let outcome =
            bridge.receive_dequeued_payload(&data, &[0x22_u8; 32], PLAN486_NOW_MS, false, false);
        assert!(matches!(outcome, DequeuedPayloadOutcome::StreamingAccepted));
        let after_mirror = bridge.receiver_streaming().data_packet_ingress();
        let after_canonical = bridge.streaming().data_packet_ingress();
        assert_eq!(
            after_mirror.delta_since(before_mirror).data_arrived,
            1,
            "the DATA must reach the mirror that owns the accepted connection"
        );
        assert_eq!(
            after_canonical.delta_since(before_canonical).data_arrived,
            0,
            "the DATA must not be routed to the canonical manager"
        );
        // The bytes are readable by the application through the
        // mirror — the delivery path the ACCEPT-side pump drains.
        let drained = bridge
            .receiver_streaming_mut()
            .drain_delivered_for(connection_id);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].bytes, b"payload".to_vec());
    }

    #[test]
    fn established_phase_shape_reports_the_send_stream_id() {
        // Plan 487 WP3: the shape peek must report the packet's
        // `send_stream_id` as the local receive stream id. A peer
        // addresses us with `sendStreamId = <our receive id>` and
        // `receiveStreamId = <the peer's own id>`; the manager
        // resolves an established-phase packet on the send field, so
        // the peek has to read the same field. Reading the receive
        // field instead reports the peer's id, which is never a key
        // in `inbound_by_stream` / `outbound_by_stream`.
        let bytes = plan487_inner_i2np_data_bytes(0x1111_2222, 0x3333_4444, b"payload");
        assert_eq!(
            inbound_payload_shape(&bytes),
            InboundPayloadShape::Established {
                local_receive_stream_id: 0x1111_2222,
            }
        );
    }

    #[test]
    fn originator_syn_shape_is_unchanged_by_the_field_contract() {
        // The originator SYN keeps its own rule: `send_stream_id == 0`
        // and `receive_stream_id != 0`. The established-phase branch
        // must not swallow it.
        let (recipient, _) = plan486_bridge();
        let mut rng = ChaCha8Rng::seed_from_u64(0x4870_0005);
        let originator = DestinationIdentity::generate(&mut rng).expect("originator");
        let mut sender = StreamingManager::new(StreamingConfig::balanced());
        let recipient_remote = plan486_remote(&recipient);
        let syn =
            plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4870_0006);
        assert_eq!(
            inbound_payload_shape(&syn),
            InboundPayloadShape::OriginatorSyn
        );
    }

    /// Plan 487 WP3 — drives one accepted (inbound) connection to the
    /// established state and returns the bridge plus the connection's
    /// two stream ids. The originator burns one stream id first so the
    /// peer's id and ours differ, exactly as they do with a real peer.
    fn plan487_established_bridge() -> (SamDestinationBridge, ConnectionId, u32, u32) {
        let (recipient, mut bridge) = plan486_bridge();
        bridge
            .receiver_streaming_mut()
            .listen(0)
            .expect("mirror wildcard listen");
        let mut rng = ChaCha8Rng::seed_from_u64(0x4870_000b);
        let originator = DestinationIdentity::generate(&mut rng).expect("originator");
        let mut sender = StreamingManager::new(StreamingConfig::balanced());
        let recipient_remote = plan486_remote(&recipient);
        let _ = plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4870_000c);
        let syn =
            plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4870_000d);
        let outcome =
            bridge.receive_dequeued_payload(&syn, &[0x22_u8; 32], PLAN486_NOW_MS, false, true);
        assert!(matches!(outcome, DequeuedPayloadOutcome::StreamingAccepted));
        let connection_id = bridge
            .receiver_streaming_mut()
            .accept(0, PLAN486_NOW_MS)
            .expect("accepted connection");
        let local_receive_stream_id = bridge
            .receiver_streaming()
            .get_connection(connection_id)
            .expect("connection")
            .local_stream_id();
        let remote_stream_id = bridge
            .receiver_streaming()
            .get_connection(connection_id)
            .expect("connection")
            .remote_stream_id();
        assert_ne!(local_receive_stream_id, remote_stream_id);
        bridge.note_established_at(PLAN486_NOW_MS);
        (
            bridge,
            connection_id,
            local_receive_stream_id,
            remote_stream_id,
        )
    }

    #[test]
    fn terminal_stall_snapshot_classifies_a_data_phase_that_never_arrived() {
        // Plan 487 WP3: the data phase ends with no DATA at either
        // manager. The terminal snapshot must fire (the window-based
        // live receipt cannot cover a phase that ends early) and it
        // must read every stage from its own counter.
        let (mut bridge, _connection_id, _local, _remote) = plan487_established_bridge();
        let snapshot = bridge
            .terminal_data_phase_stall_snapshot(PLAN486_NOW_MS, (0, 0))
            .expect("terminal stall snapshot");
        assert_eq!(snapshot.app_bytes_drained, 0);
        assert_eq!(snapshot.mirror_observed, 0);
        assert_eq!(snapshot.mirror_arrived, 0);
        assert_eq!(snapshot.canonical_observed, 0);
        assert_eq!(snapshot.canonical_arrived, 0);
        assert_eq!(snapshot.mirror_ack_generated, 0);
        assert_eq!(snapshot.canonical_ack_generated, 0);
        assert_eq!(snapshot.ack_egress_sent, 0);
        assert_eq!(snapshot.ack_egress_dropped, 0);
        // One-shot per destination: the second read is silent.
        assert!(
            bridge
                .terminal_data_phase_stall_snapshot(PLAN486_NOW_MS, (0, 0))
                .is_none()
        );
    }

    #[test]
    fn terminal_stall_snapshot_is_silent_when_data_arrived() {
        // The data phase delivered: nothing was stalled, so no receipt
        // may be emitted even though the application has not drained
        // the bytes yet.
        let (mut bridge, _connection_id, local, remote) = plan487_established_bridge();
        let data = plan487_inner_i2np_data_bytes(local, remote, b"payload");
        let outcome =
            bridge.receive_dequeued_payload(&data, &[0x22_u8; 32], PLAN486_NOW_MS, false, false);
        assert!(matches!(outcome, DequeuedPayloadOutcome::StreamingAccepted));
        assert!(
            bridge
                .terminal_data_phase_stall_snapshot(PLAN486_NOW_MS, (0, 0))
                .is_none()
        );
    }

    #[test]
    fn classifier_matrix_matches_routing_contract() {
        let (recipient, _) = plan486_bridge();
        let mut rng = ChaCha8Rng::seed_from_u64(0x4860_000d);
        let originator = DestinationIdentity::generate(&mut rng).expect("originator");
        let mut sender = StreamingManager::new(StreamingConfig::balanced());
        let recipient_remote = plan486_remote(&recipient);
        let syn =
            plan486_stock_shaped_syn(&mut sender, &originator, &recipient_remote, 0x4860_000e);
        use InboundStreamingTarget::{Canonical, Mirror};
        // Client profile: mirror iff parked and SYN-shaped.
        assert_eq!(classify_inbound_streaming_target(&syn, false, true), Mirror);
        assert_eq!(
            classify_inbound_streaming_target(&syn, false, false),
            Canonical
        );
        // Server profile: mirror regardless.
        assert_eq!(classify_inbound_streaming_target(&syn, true, false), Mirror);
        assert_eq!(classify_inbound_streaming_target(&syn, true, true), Mirror);
        // Non-SYN and undecodable: canonical on client profile even
        // when parked.
        assert_eq!(
            classify_inbound_streaming_target(&[0xFF_u8; 64], false, true),
            Canonical
        );
        assert_eq!(
            classify_inbound_streaming_target(&[], false, true),
            Canonical
        );
    }
}
