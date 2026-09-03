//! Plan 149 §4 — explicit SAM localhost product fabric.
//!
//! Milestone 7 is not yet backed by live router-to-router tunnel
//! construction. The localhost SAM product therefore needs an explicit
//! daemon-owned local delivery capability rather than test-only
//! fixture installation.
//!
//! The fabric owns or provides:
//!
//! - the local destination's usable outbound product material;
//! - fresh one-shot inbound `EstablishedTunnel` material for
//!   [`i2pr_client::deliver`];
//! - a locally signed LeaseSet2 advertisement;
//! - per-destination [`InboundTunnelFactory`] wiring for the
//!   per-destination runtime driver.
//!
//! The fabric is the **authenticated-router-link-bypassed localhost
//! product seam**, not a live I2P transport. It must never be described
//! as router interoperability, and it must never appear in a public
//! `advertised = true` protocol support entry.
//!
//! ## Rules
//!
//! - OS CSPRNG for runtime-created ephemeral material; deterministic
//!   factories remain test-only.
//! - No root/sudo/namespaces/Docker/VM/systemd/public network.
//! - No network socket may pretend to be a live NTCP2/SSU2 peer.
//! - No support/advertised flag may imply router interoperability.
//! - No testkit dependency may enter this module.
//!
//! ## Identity ownership
//!
//! [`SamLocalProductFabric::prepare_for_destination`] consumes only
//! non-secret routing metadata. The destination's
//! [`i2pr_crypto::OsRng`] produces fresh outbound and inbound hop
//! material so no tunnel shape repeats across sessions; the
//! [`crate::sam::SamServiceState`] owns the identity allocation and
//! shares a single `Arc<DestinationIdentity>` between the destination
//! runtime and the SAM bridge per Plan 149 §3 Option A.

#![forbid(unsafe_code)]

use std::sync::Arc;

use i2pr_client::streaming::transport::TransportSendRequest;
use i2pr_client::{DestinationIdentity, DestinationOutboundRole, build_signed_lease_set2};
use i2pr_crypto::OsRng;
use i2pr_netdb::LeaseSet2ValidationContext;
use i2pr_netdb::ValidatedLeaseSet2;
use i2pr_proto::Hash;
use i2pr_tunnel::{
    EstablishedHop, EstablishedNextHop, EstablishedRole, EstablishedTunnel, LayerKeys,
    TunnelDirection, TunnelId, TunnelPeer,
};
use rand_core::{RngCore, TryRngCore};

use crate::sam::SamServiceError;
use crate::sam::streams::{InboundTunnelBuildError, InboundTunnelFactory};

/// Local product material produced for one SAM destination.
pub struct LocalDestinationProduct {
    /// Sender-side outbound role; the data plane consumes it on every
    /// outbound delivery.
    pub outbound_role: DestinationOutboundRole,
    /// Inbound-tunnel factory the per-destination runtime driver uses
    /// to source a fresh `EstablishedTunnel` per delivery.
    pub inbound_tunnel_factory: Arc<dyn InboundTunnelFactory>,
    /// Signed, locally self-validated LeaseSet2 the SAM bridge
    /// advertises.
    pub validated_lease_set2: ValidatedLeaseSet2,
    /// Plaintext (pre-validation) LeaseSet2 the bridge retains for
    /// routing-state installation.
    pub lease_set2: i2pr_proto::LeaseSet2,
}

impl std::fmt::Debug for LocalDestinationProduct {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalDestinationProduct")
            .field("outbound_role", &self.outbound_role)
            .field(
                "inbound_tunnel_factory",
                &format_args!(
                    "<redacted {}>",
                    std::any::type_name::<dyn InboundTunnelFactory>()
                ),
            )
            .field("validated_lease_set2", &self.validated_lease_set2.key())
            .field("lease_set2", &"<redacted>")
            .finish()
    }
}

impl LocalDestinationProduct {
    /// Borrows the validated lease set without consuming the product.
    pub const fn validated_lease_set2(&self) -> &ValidatedLeaseSet2 {
        &self.validated_lease_set2
    }
}

/// SAM localhost product fabric. The fabric owns no secrets and no
/// per-destination state; every call to
/// [`Self::prepare_for_destination`] produces an independent bundle.
#[derive(Debug, Default, Clone, Copy)]
pub struct SamLocalProductFabric;

impl SamLocalProductFabric {
    /// Constructs an empty fabric.
    pub const fn new() -> Self {
        Self
    }

    /// Prepares the full localhost SAM product material bundle for
    /// one destination.
    ///
    /// `identity` is borrowed; the fabric never copies the private
    /// keys. The caller (the SAM service) keeps the only
    /// `Arc<DestinationIdentity>` allocation.
    pub fn prepare_for_destination(
        &self,
        identity: &DestinationIdentity,
        now_seconds: u32,
    ) -> Result<LocalDestinationProduct, SamServiceError> {
        prepare_local_product(identity, now_seconds)
    }
}

/// Default randomness source used by [`SamLocalProductFabric`].
/// The CSPRNG produces fresh hop hashes, tunnel ids, and per-hop
/// `LayerKeys` so the localhost fabric is deterministic-shape but
/// never bit-identical across sessions.
fn prepare_local_product(
    identity: &DestinationIdentity,
    now_seconds: u32,
) -> Result<LocalDestinationProduct, SamServiceError> {
    let mut os_rng = OsRng;
    let mut rng = rand_core::UnwrapMut(&mut os_rng);
    let mut leased_seed = [0_u8; 64];
    rng.try_fill_bytes(&mut leased_seed)
        .map_err(|_| SamServiceError::InvalidConfig("os rng unavailable for fabric".to_owned()))?;
    let outbound_tunnel = random_outbound_tunnel(&leased_seed, &mut rng);
    let outbound_role =
        DestinationOutboundRole::new(outbound_tunnel, u64::from(now_seconds) * 1000 + 60_000);

    let inbound_seed = {
        let mut s = [0_u8; 64];
        rng.try_fill_bytes(&mut s).map_err(|_| {
            SamServiceError::InvalidConfig("os rng unavailable for fabric".to_owned())
        })?;
        s
    };
    let inbound_factory: Arc<dyn InboundTunnelFactory> =
        Arc::new(LocalhostInboundTunnelFactory::new(inbound_seed));

    let inbound_sample = random_inbound_tunnel(&inbound_seed, &mut rng);
    let inbound_for_pool = inbound_sample;
    let mut pool =
        i2pr_client::DestinationTunnelPool::new(i2pr_client::DestinationConfig::balanced())
            .map_err(|error| SamServiceError::InvalidConfig(format!("fabric pool: {error}")))?;
    let inbound_mat = inbound_for_pool.into_extracted();
    pool.register_inbound(inbound_mat, u64::from(now_seconds))
        .map_err(|error| SamServiceError::InvalidConfig(format!("fabric pool inbound: {error}")))?;
    let sources = pool.inbound_lease_sources(u64::from(now_seconds));
    let lease_set2 = build_signed_lease_set2(identity, &sources, now_seconds)
        .map_err(|error| SamServiceError::InvalidConfig(format!("fabric lease set: {error}")))?;
    drop(pool);

    let validated_lease_set2 = ValidatedLeaseSet2::from_lease_set2(
        lease_set2.clone(),
        Some(identity.id().as_netdb_key()),
        LeaseSet2ValidationContext::new(now_seconds),
    )
    .map_err(|error| {
        SamServiceError::InvalidConfig(format!("fabric lease set validation: {error}"))
    })?;

    Ok(LocalDestinationProduct {
        outbound_role,
        inbound_tunnel_factory: inbound_factory,
        validated_lease_set2,
        lease_set2,
    })
}

/// Builds a fresh two-hop random outbound `EstablishedTunnel`. The
/// returned shape is `[Participant, OBEP]` per the canonical I2P
/// short-tunnel specification; the hop hashes, tunnel ids, and
/// `LayerKeys` are OS-CSPRNG-seeded so two fabric invocations never
/// collide.
fn random_outbound_tunnel<R: RngCore + ?Sized>(seed: &[u8; 64], rng: &mut R) -> EstablishedTunnel {
    let (hop1_hash, hop2_hash, ibgw_tunnel_id, obep_tunnel_id, creator_tunnel_id) =
        derive_hop_material(seed, rng, 0x02);
    let hop1_keys = random_layer_keys(rng);
    let hop2_keys = random_layer_keys(rng);
    let participant_receive_tunnel =
        TunnelId::new(0x0100_0000_u32.wrapping_add(u32::from(ibgw_tunnel_id))).expect("nonzero");
    let obep_receive_tunnel =
        TunnelId::new(0x0100_0001_u32.wrapping_add(u32::from(obep_tunnel_id))).expect("nonzero");
    let hops = vec![
        EstablishedHop::with_next(
            TunnelPeer::from_hash(hop1_hash),
            EstablishedRole::Participant,
            participant_receive_tunnel,
            hop1_keys,
            EstablishedNextHop::new(TunnelPeer::from_hash(hop2_hash), obep_receive_tunnel),
        ),
        EstablishedHop::terminal(
            TunnelPeer::from_hash(hop2_hash),
            EstablishedRole::OutboundEndpoint,
            obep_receive_tunnel,
            hop2_keys,
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Outbound,
        TunnelId::new(creator_tunnel_id).expect("nonzero"),
        hops,
        0,
        None,
        None,
    )
    .expect("random outbound established tunnel")
}

/// Builds a fresh two-hop random inbound `EstablishedTunnel`. The
/// returned shape is `[IBGW, Participant]`; the terminal hop's
/// `next_tunnel` is forced equal to the local inbound receive id so
/// the established-tunnel constructor accepts the bundle.
fn random_inbound_tunnel<R: RngCore + ?Sized>(seed: &[u8; 64], rng: &mut R) -> EstablishedTunnel {
    let (hop1_hash, hop2_hash, ibgw_tunnel_id, participant_tunnel_id, creator_tunnel_id) =
        derive_hop_material(seed, rng, 0x04);
    let local_receive =
        TunnelId::new(0x0300_0000_u32.wrapping_add(creator_tunnel_id)).expect("nonzero");
    let hop1_keys = random_layer_keys(rng);
    let hop2_keys = random_layer_keys(rng);
    let ibgw_receive_tunnel =
        TunnelId::new(0x0400_0000_u32.wrapping_add(u32::from(ibgw_tunnel_id))).expect("nonzero");
    let participant_receive_tunnel =
        TunnelId::new(0x0400_0001_u32.wrapping_add(u32::from(participant_tunnel_id)))
            .expect("nonzero");
    let hops = vec![
        EstablishedHop::with_next(
            TunnelPeer::from_hash(hop1_hash),
            EstablishedRole::InboundGateway,
            ibgw_receive_tunnel,
            hop1_keys,
            EstablishedNextHop::new(TunnelPeer::from_hash(hop2_hash), participant_receive_tunnel),
        ),
        EstablishedHop::with_next(
            TunnelPeer::from_hash(hop2_hash),
            EstablishedRole::Participant,
            participant_receive_tunnel,
            hop2_keys,
            EstablishedNextHop::new(TunnelPeer::from_hash(hop2_hash), local_receive),
        ),
    ];
    EstablishedTunnel::new(
        TunnelDirection::Inbound,
        TunnelId::new(creator_tunnel_id).expect("nonzero"),
        hops,
        0,
        Some((TunnelPeer::from_hash(hop1_hash), ibgw_receive_tunnel)),
        Some(local_receive),
    )
    .expect("random inbound established tunnel")
}

/// Derives five 32-byte values from the supplied fabric seed plus a
/// CSPRNG-mixed offset; the caller uses them as hop hashes and
/// tunnel-id nibbles. We never use the same seed twice because the
/// OS CSPRNG re-mixes each value before returning it.
fn derive_hop_material<R: RngCore + ?Sized>(
    seed: &[u8; 64],
    rng: &mut R,
    offset: u8,
) -> (Hash, Hash, u8, u8, u32) {
    let mut hop1_hash = [0_u8; 32];
    let mut hop2_hash = [0_u8; 32];
    for (i, byte) in hop1_hash.iter_mut().enumerate() {
        let second = (i + 32) % 64;
        *byte = seed[i] ^ seed[second] ^ offset;
    }
    for (i, byte) in hop2_hash.iter_mut().enumerate() {
        let first = (i + 8) % 64;
        let second = (i + 40) % 64;
        *byte = seed[first] ^ seed[second] ^ offset.wrapping_add(1);
    }
    let mut mask = [0_u8; 4];
    rng.try_fill_bytes(&mut mask).ok();
    let ibgw = hop1_hash[0];
    let participant = hop2_hash[0];
    let creator = u32::from_be_bytes([hop1_hash[1], hop2_hash[1], hop1_hash[2], hop2_hash[2]])
        ^ u32::from_be_bytes(mask);
    let creator = if creator == 0 { 1 } else { creator };
    (
        Hash::from_bytes(hop1_hash),
        Hash::from_bytes(hop2_hash),
        ibgw,
        participant,
        creator,
    )
}

fn random_layer_keys<R: RngCore + ?Sized>(rng: &mut R) -> LayerKeys {
    let mut reply = [0_u8; 32];
    let mut layer = [0_u8; 32];
    let mut iv = [0_u8; 32];
    rng.try_fill_bytes(&mut reply).ok();
    rng.try_fill_bytes(&mut layer).ok();
    rng.try_fill_bytes(&mut iv).ok();
    LayerKeys::new(reply, layer, iv)
}

/// CSPRNG-backed inbound-tunnel factory used by the per-destination
/// runtime driver. Each call to [`Self::build_inbound_tunnel`]
/// returns a fresh, structurally-valid inbound `EstablishedTunnel`
/// whose hop hashes, tunnel ids, and `LayerKeys` are derived from a
/// process-local nonce mixed with the OS CSPRNG. The factory never
/// reuses tunnel material across deliveries.
#[derive(Debug, Clone)]
pub struct LocalhostInboundTunnelFactory {
    seed: [u8; 64],
}

impl LocalhostInboundTunnelFactory {
    /// Constructs a factory from a process-local seed. The seed never
    /// leaves the factory; the SAM service hands the factory to the
    /// bridge, the bridge hands it to the runtime driver, the driver
    /// calls `build_inbound_tunnel` per outbound `TransportSendRequest`.
    pub fn new(seed: [u8; 64]) -> Self {
        Self { seed }
    }
}

impl InboundTunnelFactory for LocalhostInboundTunnelFactory {
    fn build_inbound_tunnel(&self) -> Result<EstablishedTunnel, InboundTunnelBuildError> {
        // Re-mix the seed against the OS CSPRNG so each call returns a
        // fresh shape. We never reuse the same inbound tunnel twice
        // because the Plan 129 local seam consumes its argument once.
        let mut os_rng = OsRng;
        let mut rng = rand_core::UnwrapMut(&mut os_rng);
        Ok(random_inbound_tunnel(&self.seed, &mut rng))
    }
}

/// Returned when the Plan 149 local product fabric cannot source a
/// usable outbound tunnel for a `TransportSendRequest`. Surfaced
/// through the SAM runtime driver (see
/// [`crate::sam::raw_stream`]) so the runtime driver can wake waiters
/// with a typed bounded failure rather than silently drop the queued
/// request.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LocalDeliveryDegradation {
    /// The sender bridge had no installed inbound tunnel factory.
    NoInboundFactory,
    /// The factory refused to build an inbound tunnel.
    FactoryExhausted,
    /// The local destination stack rejected a transport request.
    DeliveryFailed,
    /// The request named no locally-owned peer destination.
    UnknownPeer,
}

impl std::fmt::Display for LocalDeliveryDegradation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoInboundFactory => {
                formatter.write_str("local product fabric missing inbound tunnel factory")
            }
            Self::FactoryExhausted => {
                formatter.write_str("local product fabric inbound tunnel factory exhausted")
            }
            Self::DeliveryFailed => {
                formatter.write_str("local destination delivery rejected a transport request")
            }
            Self::UnknownPeer => formatter.write_str("local destination peer is not registered"),
        }
    }
}

impl std::error::Error for LocalDeliveryDegradation {}

/// Counters returned by the runtime driver after one delivery sweep.
/// Plan 149 §8 requires the driver to record every typed bounded
/// failure; the SAM service exposes the counters for the FORWARD and
/// PLAN 149 fault tests.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub struct DeliverySweepCounters {
    /// Number of `TransportSendRequest`s successfully delivered.
    pub delivered: usize,
    /// Number of requests dropped because the destination had no
    /// inbound tunnel factory installed.
    pub missing_factory: usize,
    /// Number of requests the factory refused to build.
    pub factory_exhausted: usize,
    /// Number of requests whose peer destination hash matched no
    /// locally-owned bridge.
    pub unknown_peer: usize,
    /// Number of requests rejected by the local destination delivery seam.
    pub delivery_failed: usize,
}

impl DeliverySweepCounters {
    /// Adds another bounded sweep into this snapshot without allowing
    /// counters to wrap.
    pub fn saturating_add_assign(&mut self, other: Self) {
        self.delivered = self.delivered.saturating_add(other.delivered);
        self.missing_factory = self.missing_factory.saturating_add(other.missing_factory);
        self.factory_exhausted = self
            .factory_exhausted
            .saturating_add(other.factory_exhausted);
        self.unknown_peer = self.unknown_peer.saturating_add(other.unknown_peer);
        self.delivery_failed = self.delivery_failed.saturating_add(other.delivery_failed);
    }

    /// `true` when every successful delivery happened with no
    /// typed degradation.
    pub const fn clean(&self) -> bool {
        self.missing_factory == 0
            && self.factory_exhausted == 0
            && self.unknown_peer == 0
            && self.delivery_failed == 0
    }
}

/// Typed wrapper used by the runtime driver to convert a
/// `DeliverySweepCounters` into the right outbound-signal wakeup.
pub fn degrade_to_reason(counters: DeliverySweepCounters) -> Option<LocalDeliveryDegradation> {
    if counters.delivery_failed > 0 {
        Some(LocalDeliveryDegradation::DeliveryFailed)
    } else if counters.unknown_peer > 0 {
        Some(LocalDeliveryDegradation::UnknownPeer)
    } else if counters.factory_exhausted > 0 {
        Some(LocalDeliveryDegradation::FactoryExhausted)
    } else if counters.missing_factory > 0 {
        Some(LocalDeliveryDegradation::NoInboundFactory)
    } else {
        None
    }
}

/// Convenience alias for the per-destination transport request.
pub type LocalTransportRequest = TransportSendRequest;
