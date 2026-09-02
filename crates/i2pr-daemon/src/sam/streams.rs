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

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use i2pr_client::streaming::config::StreamingConfig;
use i2pr_client::streaming::manager::StreamingManager;
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::{
    DestinationDispatcher, DestinationId, DestinationIdentity, DestinationOutboundRole,
    DestinationRouting, DestinationRoutingConfig, EciesSessionConfig, EciesSessionManager,
    LeaseSetError, LocalDeliveryError, LocalDeliveryOutcome, LocalDeliveryReceiver,
    LocalDeliverySender, deliver,
};
use i2pr_netdb::LeaseSet2Store;
use i2pr_proto::Hash;
use i2pr_proto::LeaseSet2;
use i2pr_tunnel::{EstablishedTunnel, TunnelId};
use rand_core::{CryptoRng, RngCore};

use crate::sam::SamServiceError;

/// Hard ceiling on the retained outbound queue the per-destination
/// runtime surfaces for diagnostics. The seam is the production
/// adapter path, not a captured-outbound queue, so this ceiling is
/// only a safety belt under failure.
pub const MAX_BRIDGE_DIAGNOSTIC_QUEUE: usize = 1024;

/// Plan 143 removed the captured-outbound test seam. A small
/// diagnostic queue remains so the bridge can surface what
/// the SAM STREAM driver is doing without the test-only
/// history retained in plan 138. Production STREAM traffic
/// never consults the diagnostic queue.
#[derive(Clone, Debug, Default)]
pub struct BridgeDiagnostics {
    recent_outbound: VecDeque<TransportSendRequest>,
    inbound_dispatched: u64,
    inbound_observations: u64,
}

impl BridgeDiagnostics {
    fn new() -> Self {
        Self {
            recent_outbound: VecDeque::new(),
            inbound_dispatched: 0,
            inbound_observations: 0,
        }
    }

    fn record_outbound(&mut self, request: TransportSendRequest) {
        if self.recent_outbound.len() >= MAX_BRIDGE_DIAGNOSTIC_QUEUE {
            self.recent_outbound.pop_front();
        }
        self.recent_outbound.push_back(request);
    }

    fn record_inbound_dispatch(&mut self) {
        self.inbound_dispatched = self.inbound_dispatched.saturating_add(1);
    }

    fn record_inbound_observation(&mut self) {
        self.inbound_observations = self.inbound_observations.saturating_add(1);
    }

    pub fn outbound_queue_len(&self) -> usize {
        self.recent_outbound.len()
    }

    pub fn inbound_dispatched(&self) -> u64 {
        self.inbound_dispatched
    }

    pub fn inbound_observations(&self) -> u64 {
        self.inbound_observations
    }
}

/// Per-destination SAM STREAM bridge.
#[allow(dead_code)]
pub struct SamDestinationBridge {
    identity: Arc<DestinationIdentity>,
    static_secret: [u8; i2pr_crypto::X25519_KEY_LENGTH],
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
        static_secret: [u8; i2pr_crypto::X25519_KEY_LENGTH],
        lease_set2: LeaseSet2,
        outbound_role: DestinationOutboundRole,
        now_seconds: u32,
    ) -> Self {
        Self::with_shared_identity(
            Arc::new(identity),
            static_secret,
            lease_set2,
            outbound_role,
            now_seconds,
        )
    }

    /// Builds the sender-side bridge plus the receiver-side mirror for one
    /// destination using an existing `Arc<DestinationIdentity>` allocation.
    ///
    /// Plan 149 §3 Option A: the SAM service builds one secret allocation
    /// per logical destination and shares the `Arc` with the destination
    /// runtime. The bridge never reconstructs a second private identity.
    pub fn with_shared_identity(
        identity: Arc<DestinationIdentity>,
        static_secret: [u8; i2pr_crypto::X25519_KEY_LENGTH],
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
        Self {
            identity,
            static_secret,
            lease_set2,
            streaming: StreamingManager::new(StreamingConfig::balanced()),
            routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            session_manager: EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound_role,
            dispatcher: DestinationDispatcher::new(),
            lease_set2_store: LeaseSet2Store::default(),
            receiver_dispatcher,
            receiver_session: EciesSessionManager::new(EciesSessionConfig::balanced()),
            receiver_routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            receiver_streaming: StreamingManager::new(StreamingConfig::balanced()),
            receiver_lease_set2_store: LeaseSet2Store::default(),
            receiver_now_seconds: now_seconds,
            diagnostics: BridgeDiagnostics::new(),
            inbound_tunnel_factory: None,
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

    pub const fn static_secret(&self) -> &[u8; i2pr_crypto::X25519_KEY_LENGTH] {
        &self.static_secret
    }

    pub const fn outbound_role(&self) -> &DestinationOutboundRole {
        &self.outbound_role
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
    let static_secret = *identity.static_secret_bytes();
    Ok(SamDestinationBridge::new(
        identity,
        static_secret,
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

fn dummy_outbound_tunnel() -> EstablishedTunnel {
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
/// test seam.
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

#[derive(Debug, thiserror::Error)]
pub enum SamDestinationTripleError {
    #[error("sam base64 decode failed")]
    Base64,
    #[error("destination decode failed: {0}")]
    Destination(i2pr_proto::CodecError),
    #[error("destination encryption key length mismatch")]
    StaticPublicKeyLength,
}
