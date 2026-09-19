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

use i2pr_client::streaming::config::StreamingConfig;
use i2pr_client::streaming::manager::StreamingManager;
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::{
    DestinationDispatcher, DestinationId, DestinationIdentity, DestinationOutboundRole,
    DestinationRouting, DestinationRoutingConfig, EciesSessionConfig, EciesSessionManager,
    LeaseSetError, LocalDeliveryError, LocalDeliveryOutcome, LocalDeliveryReceiver,
    LocalDeliverySender, StreamingDestinationAdapter, deliver,
};
use i2pr_crypto::OsRng;
use i2pr_netdb::LeaseSet2Store;
use i2pr_netdb::{LeaseSet2ValidationContext, ValidatedLeaseSet2};
use i2pr_proto::Hash;
use i2pr_proto::LeaseSet2;
use i2pr_tunnel::{EstablishedTunnel, TunnelId};
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
}

impl BridgeDiagnostics {
    fn new() -> Self {
        Self {
            outbound_dispatches: 0,
            inbound_dispatched: 0,
            inbound_observations: 0,
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
    outbound_role: DestinationOutboundRole,
    lease_set2: LeaseSet2,
    #[allow(dead_code)]
    validated_lease_set2: ValidatedLeaseSet2,
    inbound_receive_ids: Vec<u32>,
    outbound_expires_at_ms: u64,
    inbound_expires_at_ms: u64,
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
    /// Builds router-backed state from already-installed real
    /// material. The caller owns tunnel installation; this
    /// constructor only bundles the state and checks identity
    /// consistency at install time (see
    /// [`SamDestinationBridge::install_router_network_state`]).
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
            outbound_role,
            lease_set2,
            validated_lease_set2,
            inbound_receive_ids,
            outbound_expires_at_ms,
            inbound_expires_at_ms,
        }
    }

    /// Returns true when either the outbound or inbound material is
    /// expired relative to `now_ms`.
    pub(crate) fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.outbound_expires_at_ms || now_ms >= self.inbound_expires_at_ms
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
}

/// Plan 212 §14 — outcome of draining authenticated destination
/// payloads into canonical Streaming.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterInboundDispatchReport {
    /// Whether the Garlic envelope authenticated.
    pub garlic_authenticated: bool,
    /// Number of destination payloads dequeued via `pop_payload`.
    pub payloads_dequeued: usize,
    /// Number of payloads accepted by
    /// `StreamingDestinationAdapter::receive` into the canonical
    /// service `StreamingManager`.
    pub streaming_packets_accepted: usize,
    /// Number of payloads rejected by the adapter (non-streaming
    /// protocol counts as rejected for the streaming path; Garlic
    /// auth failures return `garlic_authenticated = false` with
    /// zero dequeued).
    pub streaming_rejected: usize,
}

/// Per-destination SAM STREAM bridge.
#[allow(dead_code)]
pub struct SamDestinationBridge {
    identity: Arc<DestinationIdentity>,
    lease_set2: LeaseSet2,
    streaming: StreamingManager,
    routing: DestinationRouting,
    session_manager: EciesSessionManager,
    outbound_role: DestinationOutboundRole,
    dispatcher: DestinationDispatcher,
    lease_set2_store: LeaseSet2Store,
    receiver_dispatcher: DestinationDispatcher,
    receiver_session: EciesSessionManager,
    receiver_routing: DestinationRouting,
    receiver_streaming: StreamingManager,
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
        Self::with_shared_identity(Arc::new(identity), lease_set2, outbound_role, now_seconds)
    }

    /// Builds the sender-side bridge plus the receiver-side mirror for one
    /// destination using an existing `Arc<DestinationIdentity>` allocation.
    ///
    /// Plan 149 §3 Option A: the SAM service builds one secret allocation
    /// per logical destination and shares the `Arc` with the destination
    /// runtime. The bridge never reconstructs a second private identity.
    pub fn with_shared_identity(
        identity: Arc<DestinationIdentity>,
        lease_set2: LeaseSet2,
        outbound_role: DestinationOutboundRole,
        now_seconds: u32,
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
            streaming: StreamingManager::new(StreamingConfig::balanced()),
            routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            session_manager: EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound_role,
            dispatcher,
            lease_set2_store: LeaseSet2Store::default(),
            receiver_dispatcher,
            receiver_session: EciesSessionManager::new(EciesSessionConfig::balanced()),
            receiver_routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            receiver_streaming: StreamingManager::new(StreamingConfig::balanced()),
            receiver_lease_set2_store: LeaseSet2Store::default(),
            receiver_now_seconds: now_seconds,
            diagnostics: BridgeDiagnostics::new(),
            inbound_tunnel_factory: None,
            router_network: None,
        }
    }

    pub fn identity(&self) -> Arc<DestinationIdentity> {
        Arc::clone(&self.identity)
    }

    pub fn identity_id(&self) -> DestinationId {
        self.identity.id()
    }

    pub const fn lease_set2(&self) -> &LeaseSet2 {
        &self.lease_set2
    }

    pub const fn outbound_role(&self) -> &DestinationOutboundRole {
        &self.outbound_role
    }

    /// Plan 208 — mutable accessor for the per-destination outbound
    /// role. Production compose paths (`bridge_to_peer` and the
    /// Plan 208 remote-route helper) own the swap-and-restore cycle
    /// that consumes the role by move; the accessor is only used
    /// inside the daemon, never exposed over the public API.
    pub fn outbound_role_mut(&mut self) -> &mut DestinationOutboundRole {
        &mut self.outbound_role
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
        let local_lease_set2 = self.lease_set2.clone();
        let mut os_rng = OsRng;
        let mut rng = UnwrapMut(&mut os_rng);
        let routing = &self.routing;
        let session = &mut self.session_manager;
        let outbound = &self.outbound_role;
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
        if material.inbound_receive_ids.is_empty() {
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
        ValidatedLeaseSet2::from_lease_set2(
            material.lease_set2.clone(),
            Some(expected_key),
            LeaseSet2ValidationContext::new(now_seconds),
        )
        .map_err(|error| format!("router network LS2 validation failed: {error:?}"))?;
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
            lease_count: state.lease_set2.leases().len(),
            // The manager fills owner registration (it owns the
            // inbound-owner registry); the bridge cannot observe it.
            inbound_owner_registered: false,
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

    /// Plan 212 §13 — clones the router-backed signed LS2 for
    /// server-side publication. Returns `None` when no
    /// router-backed state is installed. The LS2 is public
    /// material (destination + leases + signature); no secret
    /// material crosses this boundary.
    pub(crate) fn router_ls2_for_publication(&self) -> Option<LeaseSet2> {
        self.router_network
            .as_ref()
            .map(|state| state.lease_set2.clone())
    }

    /// Plan 212 §12 — borrows the router-backed outbound role for
    /// NetDB lookup/publication composition. Returns `None` when
    /// no router-backed state is installed. The borrow never moves
    /// the role; installation ownership stays with the service
    /// runtime.
    pub(crate) fn router_outbound_role_ref(&self) -> Option<&DestinationOutboundRole> {
        self.router_network
            .as_ref()
            .map(|state| &state.outbound_role)
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
        let local_lease_set2 = state.lease_set2.clone();
        let mut os_rng = OsRng;
        let mut rng = UnwrapMut(&mut os_rng);
        let routing = &state.routing;
        let session = &mut state.session_manager;
        let outbound = &state.outbound_role;
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

    /// Plan 212 §14 — decodes one recovered standard I2NP Garlic
    /// envelope against router-backed ECIES/session/routing state,
    /// drains every authenticated destination payload via
    /// `DestinationDispatcher::pop_payload(local_destination)`, and
    /// delivers each payload into the SAME canonical service
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
    ///    through the same tested path as local traffic);
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
            i2pr_client::InboundDispatchOutcome::Rejected(_) => {
                self.diagnostics.record_inbound_observation();
                return Ok(RouterInboundDispatchReport {
                    garlic_authenticated: false,
                    payloads_dequeued: 0,
                    streaming_packets_accepted: 0,
                    streaming_rejected: 0,
                });
            }
        };
        let Some(remote_hash) = remote_hash_opt else {
            self.diagnostics.record_inbound_observation();
            return Ok(RouterInboundDispatchReport {
                garlic_authenticated: true,
                payloads_dequeued: 0,
                streaming_packets_accepted: 0,
                streaming_rejected: 0,
            });
        };
        // Drain every queued payload for the local destination (not
        // just the first clove) into the SAME canonical service
        // `StreamingManager` the application pump reads.
        let mut dequeued = 0_usize;
        let mut accepted = 0_usize;
        let mut rejected = 0_usize;
        while let Some(payload) = self.dispatcher.pop_payload(local_id) {
            dequeued = dequeued.saturating_add(1);
            // Plan 213: server profiles feed the receiver mirror so
            // the polled server loop observes inbound SYNs; client
            // profiles feed the canonical manager that owns the
            // outbound SYN state.
            let receive_outcome = if is_server {
                StreamingDestinationAdapter::receive(
                    payload.bytes(),
                    &self.identity,
                    &mut self.receiver_streaming,
                    &remote_hash,
                    now_ms,
                )
            } else {
                StreamingDestinationAdapter::receive(
                    payload.bytes(),
                    &self.identity,
                    &mut self.streaming,
                    &remote_hash,
                    now_ms,
                )
            };
            match receive_outcome {
                Ok(
                    i2pr_client::streaming_adapter::InboundStreamingOutcome::StreamingDispatched {
                        ..
                    },
                ) => {
                    accepted = accepted.saturating_add(1);
                }
                Ok(
                    i2pr_client::streaming_adapter::InboundStreamingOutcome::UnsupportedProtocol {
                        protocol: _,
                    },
                ) => {
                    rejected = rejected.saturating_add(1);
                }
                Err(_) => {
                    rejected = rejected.saturating_add(1);
                }
            }
        }
        if accepted > 0 {
            self.diagnostics.record_inbound_dispatch();
        } else {
            self.diagnostics.record_inbound_observation();
        }
        Ok(RouterInboundDispatchReport {
            garlic_authenticated: true,
            payloads_dequeued: dequeued,
            streaming_packets_accepted: accepted,
            streaming_rejected: rejected,
        })
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
        self.by_id.get(&local_id).cloned()
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
        let (lease_set2, identity_key) =
            handle.with(|bridge| (bridge.lease_set2().clone(), bridge.identity_netdb_key()));
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
}

impl std::fmt::Display for BridgeDeliveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPeer(hash) => write!(formatter, "no peer bridge registered for {hash:?}"),
            Self::Delivery(error) => write!(formatter, "local delivery failed: {error}"),
            Self::NotStreaming => formatter.write_str("inbound observation not protocol 6"),
        }
    }
}

impl std::error::Error for BridgeDeliveryError {}

impl From<LocalDeliveryError> for BridgeDeliveryError {
    fn from(error: LocalDeliveryError) -> Self {
        Self::Delivery(error)
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
    // Step 1: extract the hop hashes from the inbound tunnel so we
    // can pass them to deliver() without holding the bridge lock.
    let inbound_hop1_hash = peer_inbound_tunnel
        .hops()
        .first()
        .map_or(Hash::from_bytes([0_u8; 32]), |hop| hop.peer().hash());
    let inbound_hop2_hash = peer_inbound_tunnel
        .hops()
        .get(1)
        .map_or(Hash::from_bytes([0_u8; 32]), |hop| hop.peer().hash());

    // Step 2: take the peer receiver-state fields out of the peer
    // bridge, build the LocalDeliveryReceiver/LocalDeliverySender
    // bundles, run deliver(), then move the fields back into the
    // bridge. The bridge fields are not `mut` at the struct level,
    // so we have to swap them with empty placeholders, run the
    // delivery, then swap them back.
    let mut receiver_dispatcher = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(&mut peer_guard.receiver_dispatcher, empty_dispatcher())
    };
    let mut receiver_session = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.receiver_session,
            EciesSessionManager::new(EciesSessionConfig::balanced()),
        )
    };
    let mut receiver_routing = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.receiver_routing,
            DestinationRouting::new(DestinationRoutingConfig::balanced()),
        )
    };
    let mut receiver_streaming = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.receiver_streaming,
            StreamingManager::new(StreamingConfig::balanced()),
        )
    };
    let mut peer_canonical_streaming = {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        std::mem::replace(
            &mut peer_guard.streaming,
            StreamingManager::new(StreamingConfig::balanced()),
        )
    };
    let mut receiver_lease_set2_store = {
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
        std::mem::replace(
            &mut sender_guard.outbound_role,
            DestinationOutboundRole::new(dummy_outbound_tunnel(), 0),
        )
    };
    let sender_lease_set2 = {
        let sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        sender_guard.lease_set2.clone()
    };
    let mut sender_routing = {
        let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        std::mem::replace(
            &mut sender_guard.routing,
            DestinationRouting::new(DestinationRoutingConfig::balanced()),
        )
    };
    let mut sender_session = {
        let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        std::mem::replace(
            &mut sender_guard.session_manager,
            EciesSessionManager::new(EciesSessionConfig::balanced()),
        )
    };

    let mut sender_inputs = LocalDeliverySender {
        identity: &sender_identity_arc,
        routing: &mut sender_routing,
        session: &mut sender_session,
        outbound: &sender_outbound_role,
        local_lease_set2: &sender_lease_set2,
        now_seconds,
        now_ms,
    };
    let mut receiver_inputs = LocalDeliveryReceiver {
        identity: &identity_arc,
        dispatcher: &mut receiver_dispatcher,
        session: &mut receiver_session,
        routing: &mut receiver_routing,
        streaming: &mut receiver_streaming,
        canonical_streaming: Some(&mut peer_canonical_streaming),
        lease_set2_store: &mut receiver_lease_set2_store,
        now_seconds: receiver_now_seconds,
    };

    let outcome = deliver(
        request,
        &mut sender_inputs,
        &mut receiver_inputs,
        outbound_hop0_hash,
        outbound_hop1_hash,
        peer_inbound_tunnel,
        inbound_hop1_hash,
        inbound_hop2_hash,
        outbound_tunnel_id,
        rng,
    );

    // Restore the moved fields back into their owning bridges.
    //
    // Plan 149 §7: the receiver routing was extracted from the peer's
    // CANONICAL `routing` field, so the modified routing (with the
    // freshly installed remote LeaseSet2) must land back in the
    // canonical field. The original `receiver_routing` mirror field
    // is untouched by this call and stays where it was.
    {
        let mut sender_guard = sender.inner.lock().expect("sender bridge poisoned");
        sender_guard.record_outbound_dispatch(request.clone());
        sender_guard.routing = sender_routing;
        sender_guard.session_manager = sender_session;
        sender_guard.outbound_role = sender_outbound_role;
    }
    {
        let mut peer_guard = peer.inner.lock().expect("peer bridge poisoned");
        peer_guard.record_inbound_dispatch();
        peer_guard.receiver_dispatcher = receiver_dispatcher;
        peer_guard.receiver_session = receiver_session;
        peer_guard.routing = receiver_routing;
        peer_guard.receiver_streaming = receiver_streaming;
        peer_guard.streaming = peer_canonical_streaming;
        peer_guard.receiver_lease_set2_store = receiver_lease_set2_store;
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
        LocalDeliveryOutcome::DispatchRejected(_) => Ok(()),
    }
}

fn empty_dispatcher() -> DestinationDispatcher {
    DestinationDispatcher::new()
}

pub(crate) fn dummy_outbound_tunnel() -> EstablishedTunnel {
    use i2pr_tunnel::{
        EstablishedHop, EstablishedNextHop, EstablishedRole, LayerKeys, TunnelDirection,
    };
    let hash = Hash::from_bytes([0xB1; 32]);
    let hop1 = EstablishedHop::with_next(
        i2pr_tunnel::TunnelPeer::from_hash(hash),
        EstablishedRole::Participant,
        TunnelId::new(0x6000_0000).expect("id"),
        LayerKeys::new([0; 32], [0; 32], [0; 32]),
        EstablishedNextHop::new(
            i2pr_tunnel::TunnelPeer::from_hash(Hash::from_bytes([0xB2; 32])),
            TunnelId::new(0x6000_0001).expect("id"),
        ),
    );
    let hop2 = EstablishedHop::terminal(
        i2pr_tunnel::TunnelPeer::from_hash(Hash::from_bytes([0xB2; 32])),
        EstablishedRole::OutboundEndpoint,
        TunnelId::new(0x6000_0001).expect("id"),
        LayerKeys::new([0; 32], [0; 32], [0; 32]),
    );
    EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(0x6000_0010).expect("id"),
        vec![hop1, hop2],
        0,
        None,
        None,
    )
    .expect("dummy outbound tunnel")
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
