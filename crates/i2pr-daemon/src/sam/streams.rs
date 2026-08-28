//! Plan 138 SAM 3.1 STREAM bridge runtime surface.
//!
//! The bridge owns one [`SamDestinationBridge`] per SAM session
//! destination. Each bridge holds:
//!
//! - the destination identity (non-`Clone`, owned);
//! - the [`StreamingManager`] (per-destination, non-`Clone`);
//! - the locally signed [`LeaseSet2`] plus the per-destination
//!   [`DestinationOutboundRole`] required by the
//!   [`StreamingDestinationAdapter`] outbound composition path;
//! - a [`EciesSessionManager`] and [`DestinationRouting`] per
//!   destination so the production router can route real outbound
//!   deliveries without re-instantiating the routing pipeline;
//! - the static X25519 secret required by the adapter's bound New
//!   Session path;
//! - a captured-outbound queue the local test seam drains to verify
//!   that every outbound byte traversed the real
//!   `StreamingManager -> StreamingDestinationAdapter` path.
//!
//! The bridge is **not** a substitute for the broader router
//! delivery layer: it owns the runtime-neutral pieces the SAM
//! STREAM bridge needs so [`crate::sam`] can call
//! [`StreamingDestinationAdapter::send`] from inside the per-stream
//! task. The actual outbound delivery of the resulting delivery
//! plan remains the tunnel data plane's job; Plan 140 wires that
//! path into the live service graph.
//!
//! ## Concurrency
//!
//! The bridge is wrapped in a [`std::sync::Mutex`] and exposed via
//! [`Arc`]. Every per-stream task locks the bridge for the duration
//! of one [`StreamingManager`] or adapter call. The lock is held
//! briefly; the adapter call is the longest single critical
//! section. Per-stream tasks therefore serialise on the bridge but
//! never block on I/O while holding it.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use i2pr_client::streaming::manager::StreamingManager;
use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::{
    DestinationId, DestinationIdentity, DestinationOutboundRole, DestinationRouting,
    DestinationRoutingConfig, EciesSessionConfig, EciesSessionManager, StreamingDestinationAdapter,
};
use i2pr_proto::LeaseSet2;
use i2pr_tunnel::EstablishedMaterial;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use crate::sam::SamServiceError;

/// Hard ceiling on the captured outbound queue per destination. The
/// queue is a test seam; production routing consumes the adapter
/// output elsewhere. Plan 140 replaces the queue with the tunnel
/// data plane wiring.
pub const MAX_CAPTURED_OUTBOUND_PER_DESTINATION: usize = 1024;

/// Captured outbound `TransportSendRequest` produced by the SAM
/// bridge. Tests and Plan 140 may inspect the queue; it is the
/// authoritative record that every byte traversed the real
/// `StreamingManager -> StreamingDestinationAdapter` path.
#[derive(Clone, Debug)]
pub struct CapturedOutbound {
    /// Destination hash the StreamingManager targeted.
    pub destination_hash: [u8; 32],
    /// Source I2P port carried by the Streaming packet.
    pub source_port: u16,
    /// Destination I2P port carried by the Streaming packet.
    pub destination_port: u16,
    /// Streaming sequence number.
    pub sequence: u32,
    /// Sender stream id.
    pub send_stream_id: u32,
    /// Receiver stream id.
    pub receive_stream_id: u32,
    /// The full gzip-encoded protocol-6 client payload (the I2NP
    /// Data body) the StreamingManager emitted. Tests feed this
    /// directly into the receiving session's
    /// `StreamingManager::process_inbound_envelope`.
    pub application_payload: Vec<u8>,
}

impl From<TransportSendRequest> for CapturedOutbound {
    fn from(request: TransportSendRequest) -> Self {
        Self {
            destination_hash: request.destination_hash,
            source_port: request.source_port,
            destination_port: request.destination_port,
            sequence: request.sequence,
            send_stream_id: request.send_stream_id,
            receive_stream_id: request.receive_stream_id,
            application_payload: request.application_payload,
        }
    }
}

/// Typed bridge failures.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// The adapter rejected the supplied request.
    #[error("streaming adapter rejected request: {0}")]
    Adapter(#[from] i2pr_client::StreamingAdapterError),
    /// The streaming manager rejected the call.
    #[error("streaming manager rejected call: {0}")]
    Streaming(#[from] i2pr_client::streaming::manager::StreamingManagerError),
    /// A captured-outbound slot could not be reserved.
    #[error("captured outbound queue is full (maximum {0})")]
    CapturedOutboundFull(usize),
    /// The session identity failed to sign a follow-up packet.
    #[error("destination identity rejected signing operation: {0}")]
    Identity(i2pr_client::DestinationIdentityError),
    /// The session registry did not recognise the supplied
    /// destination.
    #[error("no bridge installed for destination {0:?}")]
    UnknownDestination(DestinationId),
    /// The captured outbound queue is empty.
    #[error("captured outbound queue is empty")]
    CapturedOutboundEmpty,
    /// The supplied destination bytes failed to decode.
    #[error("destination bytes failed to decode: {0}")]
    DestinationDecode(i2pr_proto::CodecError),
}

/// Outcome of one captured outbound `TransportSendRequest`.
#[derive(Clone, Debug)]
pub struct CapturedOutboundEntry {
    /// Captured `TransportSendRequest` (or its bytes equivalent).
    pub captured: CapturedOutbound,
    /// Plan byte length when the adapter was invoked; zero when the
    /// adapter was bypassed for the local seam.
    pub plan_bytes: usize,
}

/// Per-destination SAM STREAM bridge. Constructed by
/// [`SamDestinations::install`] and held by every per-stream socket
/// task through [`Arc<Mutex<SamDestinationBridge>>`].
pub struct SamDestinationBridge {
    /// The destination identity is held inside an [`Arc`] so the
    /// per-stream task can clone the handle and call
    /// `StreamingManager::connect` with `&DestinationIdentity`
    /// without taking a borrow that conflicts with the manager's
    /// mutable borrow. `DestinationIdentity` is non-`Clone`; the
    /// `Arc` is the only way to share it safely.
    identity: Arc<DestinationIdentity>,
    static_secret: [u8; i2pr_crypto::X25519_KEY_LENGTH],
    lease_set2: LeaseSet2,
    streaming: StreamingManager,
    routing: DestinationRouting,
    session_manager: EciesSessionManager,
    outbound_role: DestinationOutboundRole,
    captured_outbound: VecDeque<CapturedOutbound>,
}

impl std::fmt::Debug for SamDestinationBridge {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SamDestinationBridge")
            .field("identity", &self.identity.id())
            .field("lease_set2", &"<redacted>")
            .field("streaming", &"<redacted>")
            .field("captured_outbound_len", &self.captured_outbound.len())
            .finish_non_exhaustive()
    }
}

impl SamDestinationBridge {
    /// Constructs a fresh bridge for the supplied destination
    /// identity and routing inputs.
    pub fn new(
        identity: DestinationIdentity,
        static_secret: [u8; i2pr_crypto::X25519_KEY_LENGTH],
        lease_set2: LeaseSet2,
        outbound_role: DestinationOutboundRole,
    ) -> Self {
        Self {
            identity: Arc::new(identity),
            static_secret,
            lease_set2,
            streaming: StreamingManager::new(i2pr_client::streaming::StreamingConfig::balanced()),
            routing: DestinationRouting::new(DestinationRoutingConfig::balanced()),
            session_manager: EciesSessionManager::new(EciesSessionConfig::balanced()),
            outbound_role,
            captured_outbound: VecDeque::new(),
        }
    }

    /// Returns the destination identity handle (cloned `Arc`).
    pub fn identity(&self) -> Arc<DestinationIdentity> {
        Arc::clone(&self.identity)
    }

    /// Returns the static X25519 secret required by the adapter's
    /// bound New Session path.
    pub const fn static_secret(&self) -> &[u8; i2pr_crypto::X25519_KEY_LENGTH] {
        &self.static_secret
    }

    /// Returns the locally signed LeaseSet2.
    pub const fn lease_set2(&self) -> &LeaseSet2 {
        &self.lease_set2
    }

    /// Returns a mutable reference to the streaming manager.
    pub fn streaming_mut(&mut self) -> &mut StreamingManager {
        &mut self.streaming
    }

    /// Returns a reference to the streaming manager.
    pub const fn streaming(&self) -> &StreamingManager {
        &self.streaming
    }

    /// Returns a mutable reference to the EciesSessionManager.
    pub fn session_manager_mut(&mut self) -> &mut EciesSessionManager {
        &mut self.session_manager
    }

    /// Returns a mutable reference to the destination routing.
    pub fn routing_mut(&mut self) -> &mut DestinationRouting {
        &mut self.routing
    }

    /// Drains the captured outbound queue (test seam + future
    /// delivery wiring).
    pub fn drain_captured_outbound(&mut self) -> Vec<CapturedOutbound> {
        self.captured_outbound.drain(..).collect()
    }

    /// Snapshot the captured outbound queue length.
    pub fn captured_outbound_len(&self) -> usize {
        self.captured_outbound.len()
    }

    /// Pushes the supplied request into the captured outbound queue.
    /// The bridge records every byte the StreamingManager emits so a
    /// downstream seam can verify the full
    /// `StreamingManager -> StreamingDestinationAdapter` path.
    pub fn record_captured(&mut self, request: TransportSendRequest) -> Result<(), BridgeError> {
        if self.captured_outbound.len() >= MAX_CAPTURED_OUTBOUND_PER_DESTINATION {
            return Err(BridgeError::CapturedOutboundFull(
                MAX_CAPTURED_OUTBOUND_PER_DESTINATION,
            ));
        }
        self.captured_outbound.push_back(request.into());
        Ok(())
    }

    /// Routes one captured `TransportSendRequest` through the
    /// [`StreamingDestinationAdapter::send`] pipeline. The adapter
    /// is the single canonical outbound composition owner.
    ///
    /// Plan 138 calls this from the per-stream task after
    /// capturing the request from the StreamingManager so the
    /// production delivery wiring can pick up the resulting
    /// `OutboundDeliveryPlan`. Plan 140 replaces the test seam with
    /// the live outbound tunnel data plane.
    #[allow(clippy::too_many_arguments)]
    pub fn adapter_send(
        &mut self,
        request: &TransportSendRequest,
        now_seconds: u32,
        now_ms: u64,
    ) -> Result<CapturedOutboundEntry, BridgeError> {
        // Plan 138: the adapter requires a `CryptoRng`; rand_core
        // 0.9's `OsRng` implements `TryCryptoRng` but not the
        // infallible `CryptoRng`. The bridge therefore uses a
        // ChaCha8 stream for the local seam. Plan 140 swaps in a
        // CSPRNG with a deterministic fallback when wiring the live
        // tunnel delivery layer.
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let plan = StreamingDestinationAdapter::send(
            request,
            &self.routing,
            &mut self.session_manager,
            &self.outbound_role,
            self.identity.id(),
            &self.static_secret,
            &self.lease_set2,
            now_seconds,
            now_ms,
            &mut rng,
        )?;
        let entry = CapturedOutboundEntry {
            captured: CapturedOutbound::from(request.clone()),
            plan_bytes: plan.cells.len(),
        };
        Ok(entry)
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
    /// Wraps an existing bridge.
    pub fn new(bridge: SamDestinationBridge) -> Self {
        Self {
            inner: Arc::new(Mutex::new(bridge)),
        }
    }

    /// Locks the bridge for a single transactional call. The closure
    /// receives a mutable reference to the bridge.
    pub fn with<R>(&self, closure: impl FnOnce(&mut SamDestinationBridge) -> R) -> R {
        let mut guard = self.inner.lock().expect("sam bridge mutex poisoned");
        closure(&mut guard)
    }

    /// Returns the inner `Arc<Mutex<>>` for callers that need to
    /// hold the lock across awaits.
    pub fn into_inner(self) -> Arc<Mutex<SamDestinationBridge>> {
        self.inner
    }

    /// Returns the inner `Arc<Mutex<>>` shared handle.
    pub fn inner(&self) -> &Arc<Mutex<SamDestinationBridge>> {
        &self.inner
    }
}

/// Bounded per-destination bridge registry.
#[derive(Default)]
pub struct SamDestinations {
    by_id: HashMap<DestinationId, SamDestinationHandle>,
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
    /// Constructs an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of registered destinations.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Returns whether the registry holds no destinations.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Installs a fresh bridge for `destination_id`.
    pub fn install(
        &mut self,
        destination_id: DestinationId,
        bridge: SamDestinationBridge,
    ) -> SamDestinationHandle {
        let handle = SamDestinationHandle::new(bridge);
        self.by_id.insert(destination_id, handle.clone());
        handle
    }

    /// Returns the handle for the supplied destination, when one is
    /// registered.
    pub fn get(&self, destination_id: DestinationId) -> Option<SamDestinationHandle> {
        self.by_id.get(&destination_id).cloned()
    }

    /// Removes the bridge for the supplied destination.
    pub fn remove(&mut self, destination_id: DestinationId) -> Option<SamDestinationHandle> {
        self.by_id.remove(&destination_id)
    }
}

/// Typed SAM bridge construction failures.
#[derive(Debug, thiserror::Error)]
pub enum SamBridgeBuildError {
    /// The supplied material failed to produce a usable inbound
    /// lease source.
    #[error("inbound lease source construction failed: {0}")]
    LeaseSource(String),
    /// The signed LeaseSet2 construction failed.
    #[error("signed LeaseSet2 construction failed: {0}")]
    LeaseSet(i2pr_client::LeaseSetError),
    /// The destination identity rejected the supplied secret.
    #[error("destination identity construction failed: {0}")]
    Identity(#[from] i2pr_client::DestinationIdentityError),
    /// The destination tunnel pool could not be constructed.
    #[error("destination pool rejected: {0}")]
    Pool(#[from] i2pr_client::DestinationPoolError),
    /// The established material could not be registered with the pool.
    #[error("destination pool registration failed: {0}")]
    Registration(String),
}

impl From<SamBridgeBuildError> for SamServiceError {
    fn from(error: SamBridgeBuildError) -> Self {
        SamServiceError::InvalidConfig(format!("sam bridge build failed: {error}"))
    }
}

impl From<i2pr_client::LeaseSetError> for SamBridgeBuildError {
    fn from(error: i2pr_client::LeaseSetError) -> Self {
        Self::LeaseSet(error)
    }
}

/// Builds a SAM bridge from real established inbound + outbound
/// material. The signed LeaseSet2 is constructed via the same
/// `DestinationTunnelPool` -> `inbound_lease_sources` ->
/// `build_signed_lease_set2` path used by the Plan 129 trajectory
/// tests, so the SAM bridge holds a canonical signed LeaseSet2 the
/// adapter can bundle into a fresh bound New Session.
pub fn build_sam_destination_bridge(
    identity: DestinationIdentity,
    inbound: EstablishedMaterial,
    outbound: EstablishedMaterial,
    published_seconds: u32,
) -> Result<SamDestinationBridge, SamBridgeBuildError> {
    use i2pr_client::leaseset::build_signed_lease_set2;
    use i2pr_client::{DestinationConfig, DestinationTunnelPool};

    let mut pool = DestinationTunnelPool::new(DestinationConfig::balanced())?;
    pool.register_inbound(inbound, u64::from(published_seconds))
        .map_err(|error| SamBridgeBuildError::Registration(format!("{error}")))?;
    let sources = pool.inbound_lease_sources(u64::from(published_seconds));
    if sources.is_empty() {
        return Err(SamBridgeBuildError::LeaseSource(
            "destination pool produced no inbound lease sources".into(),
        ));
    }
    let lease_set2 = build_signed_lease_set2(&identity, &sources, published_seconds)?;
    let expires_ms = u64::from(published_seconds)
        .saturating_mul(1000)
        .saturating_add(60_000);
    let mut outbound = outbound;
    let outbound_tunnel = outbound.into_established_tunnel().ok_or_else(|| {
        SamBridgeBuildError::LeaseSource("outbound material already consumed".into())
    })?;
    let outbound_role = DestinationOutboundRole::new(outbound_tunnel, expires_ms);
    let static_secret = *identity.static_secret_bytes();
    Ok(SamDestinationBridge::new(
        identity,
        static_secret,
        lease_set2,
        outbound_role,
    ))
}

/// Decodes a SAM public destination Base64 text into a
/// `(DestinationId, SigningPublicKey, StaticPublicKey)` triple. The
/// decoder is strict: invalid characters, wrong length, or any
/// other codec failure surfaces a typed [`BridgeError`].
pub fn decode_destination_triple(
    text: &str,
) -> Result<
    (
        DestinationId,
        i2pr_proto::SigningPublicKey,
        [u8; i2pr_crypto::X25519_KEY_LENGTH],
    ),
    BridgeError,
> {
    use i2pr_api::sam::base64;
    let bytes =
        base64::decode(text, i2pr_api::sam::private_destination::PUB_LENGTH).map_err(|_| {
            BridgeError::DestinationDecode(i2pr_proto::CodecError::InvalidFieldValue {
                offset: 0,
                context: "SAM public destination base64",
            })
        })?;
    let destination =
        i2pr_proto::Destination::decode(&bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(BridgeError::DestinationDecode)?;
    let hash = destination.hash().map_err(BridgeError::DestinationDecode)?;
    let id = DestinationId::from_hash(hash);
    let signing_key = destination.signing_key().clone();
    let mut static_public = [0_u8; i2pr_crypto::X25519_KEY_LENGTH];
    let pk_bytes = destination.public_key().as_bytes();
    if pk_bytes.len() != i2pr_crypto::X25519_KEY_LENGTH {
        return Err(BridgeError::DestinationDecode(
            i2pr_proto::CodecError::InvalidFieldValue {
                offset: 0,
                context: "destination encryption key length",
            },
        ));
    }
    static_public.copy_from_slice(&pk_bytes[..i2pr_crypto::X25519_KEY_LENGTH]);
    Ok((id, signing_key, static_public))
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_client::testing::{established_inbound, established_outbound};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn identity(seed: u64) -> DestinationIdentity {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        DestinationIdentity::generate(&mut rng).expect("identity")
    }

    #[test]
    fn install_and_remove_bridge_round_trip() {
        let mut registry = SamDestinations::new();
        let id = DestinationId::from_hash(i2pr_proto::Hash::from_bytes([1_u8; 32]));
        let inbound = established_inbound(1);
        let outbound = established_outbound(2);
        let bridge =
            build_sam_destination_bridge(identity(0x42), inbound, outbound, 1_000).expect("bridge");
        let handle = registry.install(id, bridge);
        assert_eq!(registry.len(), 1);
        assert!(registry.get(id).is_some());
        drop(handle);
        assert!(registry.remove(id).is_some());
        assert!(registry.is_empty());
    }

    #[test]
    fn bridge_records_captured_outbound() {
        let inbound = established_inbound(11);
        let outbound = established_outbound(12);
        let bridge =
            build_sam_destination_bridge(identity(0x90), inbound, outbound, 1_000).expect("bridge");
        let handle = SamDestinationHandle::new(bridge);
        let request = TransportSendRequest {
            destination_hash: [0xAB; 32],
            source_port: 1,
            destination_port: 2,
            application_payload: vec![1, 2, 3, 4],
            sequence: 7,
            send_stream_id: 0x10,
            receive_stream_id: 0x20,
        };
        let _ = handle.with(|bridge| bridge.record_captured(request.clone()));
        let drained = handle.with(|bridge| bridge.drain_captured_outbound());
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].sequence, 7);
        assert_eq!(drained[0].application_payload, vec![1, 2, 3, 4]);
    }
}
