//! Plan 117 bounded runtime-side data-plane role registry.
//!
//! The registry is the explicit owner of activated local roles:
//!
//! - one outbound slot per [`TunnelSlot`] maps to one
//!   [`OutboundGatewayRole`];
//! - one local receive tunnel id maps to one
//!   [`LocalInboundEndpointRole`].
//!
//! The registry holds secret material exclusively inside the role
//! constructors; the `LayerKeys` are not cloned outside the role.
//! When the pool evicts a slot (expiry, mark_failed) the
//! corresponding role must be removed from the registry so the
//! runtime cannot target an expired tunnel.
//!
//! The registry is bounded by the existing
//! `ExploratoryPoolConfig.max_inbound` / `max_outbound` ceilings;
//! no independent capacity is introduced.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use crate::established::EstablishedTunnel;
use crate::identity::TunnelId;
use crate::pool::TunnelSlot;
use crate::roles::{LocalInboundEndpointRole, OutboundGatewayRole};

/// Bounded per-role capacity ceiling derived from the existing
/// pool configuration. The values match the exploratory pool's
/// inbound / outbound ceilings; the registry never holds more
/// activated roles than the pool can register.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataPlaneCapacity {
    /// Maximum number of activated outbound roles.
    pub outbound: u8,
    /// Maximum number of activated inbound endpoint roles.
    pub inbound: u8,
}

impl DataPlaneCapacity {
    /// Constructs a capacity from the supplied bounds. Both
    /// values must be non-zero.
    pub const fn new(outbound: u8, inbound: u8) -> Self {
        Self { outbound, inbound }
    }
}

/// Failure categories for [`DataPlaneRegistry`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// The registry reached its outbound capacity.
    OutboundFull,
    /// The registry reached its inbound capacity.
    InboundFull,
    /// The supplied slot is already bound to an outbound role.
    DuplicateOutbound(TunnelSlot),
    /// The supplied local receive tunnel id is already bound to an
    /// inbound role.
    DuplicateInbound(TunnelId),
    /// The supplied tunnel direction does not match the registry
    /// entry. The caller must activate outbound tunnels in the
    /// outbound map and inbound tunnels in the inbound map.
    DirectionMismatch,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutboundFull => formatter.write_str("outbound role capacity reached"),
            Self::InboundFull => formatter.write_str("inbound role capacity reached"),
            Self::DuplicateOutbound(slot) => {
                write!(formatter, "outbound slot {slot} is already bound")
            }
            Self::DuplicateInbound(id) => {
                write!(formatter, "inbound receive tunnel {id} is already bound")
            }
            Self::DirectionMismatch => formatter.write_str("tunnel direction does not match registry entry"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Bounded runtime-side registry for activated local roles. The
/// registry keeps the public registration metadata of the pool so
/// inbound reply-path selection and outbound first-hop delivery
/// metadata remain usable after activation.
#[derive(Debug)]
pub struct DataPlaneRegistry {
    capacity: DataPlaneCapacity,
    outbound: BTreeMap<TunnelSlot, OutboundGatewayRole>,
    inbound: BTreeMap<TunnelId, LocalInboundEndpointRole>,
    /// Public metadata of activated outbound slots: first hop
    /// router hash + receive tunnel id. Retained after activation
    /// so the registry can serve `outbound_first_hop` lookups
    /// without cloning the role's secret material.
    outbound_first_hop: BTreeMap<TunnelSlot, (i2pr_proto::Hash, TunnelId)>,
    /// Public metadata of activated inbound slots: first remote
    /// IBGW router hash + receive tunnel id. Retained after
    /// activation so the registry can serve reply-path selection
    /// without cloning the role's secret material.
    inbound_first_hop: BTreeMap<TunnelId, i2pr_proto::Hash>,
}

impl DataPlaneRegistry {
    /// Constructs a registry with the supplied capacity. The
    /// registry starts empty; the pool integration lives in
    /// `i2pr-daemon`.
    pub fn new(capacity: DataPlaneCapacity) -> Self {
        Self {
            capacity,
            outbound: BTreeMap::new(),
            inbound: BTreeMap::new(),
            outbound_first_hop: BTreeMap::new(),
            inbound_first_hop: BTreeMap::new(),
        }
    }

    /// Returns the configured capacity.
    pub const fn capacity(&self) -> DataPlaneCapacity {
        self.capacity
    }

    /// Returns the number of active outbound roles.
    pub fn outbound_len(&self) -> usize {
        self.outbound.len()
    }

    /// Returns the number of active inbound roles.
    pub fn inbound_len(&self) -> usize {
        self.inbound.len()
    }

    /// Activates an established outbound tunnel and binds it to
    /// the supplied slot. Returns the role the data plane can
    /// consume.
    pub fn activate_outbound(
        &mut self,
        slot: TunnelSlot,
        established: EstablishedTunnel,
        expires_at_ms: u64,
    ) -> Result<&OutboundGatewayRole, RegistryError> {
        if self.outbound.contains_key(&slot) {
            return Err(RegistryError::DuplicateOutbound(slot));
        }
        if self.outbound.len() >= self.capacity.outbound as usize {
            return Err(RegistryError::OutboundFull);
        }
        if established.direction() != crate::identity::TunnelDirection::Outbound {
            return Err(RegistryError::DirectionMismatch);
        }
        let first_hop = established.first_hop_router().hash();
        let receive_tunnel = established.first_hop_receive_tunnel();
        let role = OutboundGatewayRole::new(established, expires_at_ms);
        self.outbound_first_hop.insert(slot, (first_hop, receive_tunnel));
        self.outbound.insert(slot, role);
        Ok(self.outbound.get(&slot).expect("just inserted"))
    }

    /// Activates an established inbound tunnel and binds it to
    /// the local inbound receive tunnel id. Returns the role the
    /// data plane can consume.
    pub fn activate_inbound(
        &mut self,
        established: EstablishedTunnel,
        reassembler_capacity: usize,
        reassembler_aggregate_bytes: usize,
        reassembly_expiry_ms: u64,
        now_ms: u64,
        expires_at_ms: u64,
    ) -> Result<&LocalInboundEndpointRole, RegistryError> {
        if established.direction() != crate::identity::TunnelDirection::Inbound {
            return Err(RegistryError::DirectionMismatch);
        }
        let local_receive = established.local_inbound_receive();
        if self.inbound.contains_key(&local_receive) {
            return Err(RegistryError::DuplicateInbound(local_receive));
        }
        if self.inbound.len() >= self.capacity.inbound as usize {
            return Err(RegistryError::InboundFull);
        }
        let ibgw_router = established.first_hop_router().hash();
        let role = LocalInboundEndpointRole::new(
            established,
            reassembler_capacity,
            reassembler_aggregate_bytes,
            reassembly_expiry_ms,
            now_ms,
            expires_at_ms,
        );
        self.inbound_first_hop.insert(local_receive, ibgw_router);
        self.inbound.insert(local_receive, role);
        Ok(self.inbound.get(&local_receive).expect("just inserted"))
    }

    /// Returns the first remote IBGW router hash bound to the
    /// supplied local receive tunnel id, when one exists. The
    /// reply-path selector may use this to populate
    /// `ReplyPath` tokens without retaining secret material.
    pub fn inbound_first_hop(&self, local_receive: TunnelId) -> Option<i2pr_proto::Hash> {
        self.inbound_first_hop.get(&local_receive).copied()
    }

    /// Returns the outbound first hop router hash and receive
    /// tunnel id bound to the supplied slot.
    pub fn outbound_first_hop(&self, slot: TunnelSlot) -> Option<(i2pr_proto::Hash, TunnelId)> {
        self.outbound_first_hop.get(&slot).copied()
    }

    /// Borrows the outbound role bound to the supplied slot.
    pub fn outbound(&self, slot: TunnelSlot) -> Option<&OutboundGatewayRole> {
        self.outbound.get(&slot)
    }

    /// Borrows the inbound endpoint role bound to the supplied
    /// local receive tunnel id.
    pub fn inbound(&self, local_receive: TunnelId) -> Option<&LocalInboundEndpointRole> {
        self.inbound.get(&local_receive)
    }

    /// Mutably borrows the inbound endpoint role bound to the
    /// supplied local receive tunnel id.
    pub fn inbound_mut(
        &mut self,
        local_receive: TunnelId,
    ) -> Option<&mut LocalInboundEndpointRole> {
        self.inbound.get_mut(&local_receive)
    }

    /// Removes the outbound role bound to the supplied slot.
    /// Returns the role so the caller can drop it (which zeroizes
    /// the secret material).
    pub fn remove_outbound(&mut self, slot: TunnelSlot) -> Option<OutboundGatewayRole> {
        let role = self.outbound.remove(&slot);
        self.outbound_first_hop.remove(&slot);
        role
    }

    /// Removes the inbound role bound to the supplied local
    /// receive tunnel id. Returns the role so the caller can drop
    /// it (which zeroizes the secret material).
    pub fn remove_inbound(&mut self, local_receive: TunnelId) -> Option<LocalInboundEndpointRole> {
        let role = self.inbound.remove(&local_receive);
        self.inbound_first_hop.remove(&local_receive);
        role
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_crypto::LayerKeys;
    use crate::config::ExploratoryPoolConfig;
    use crate::established::{EstablishedHop, EstablishedNextHop, EstablishedRole};
    use crate::identity::{TunnelDirection, TunnelPeer};
    use i2pr_proto::Hash;

    fn keys() -> LayerKeys {
        LayerKeys::new([0xAA_u8; 32], [0xBB_u8; 32], [0xCC_u8; 32])
    }

    fn peer(value: u8) -> TunnelPeer {
        TunnelPeer::from_hash(Hash::from_bytes([value; 32]))
    }

    fn outbound_established(creator_id: u32) -> EstablishedTunnel {
        let hops = vec![EstablishedHop::terminal(
            peer(0x10),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(creator_id + 1).expect("id"),
            keys(),
        )];
        EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(creator_id).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("outbound established")
    }

    fn inbound_established() -> (TunnelId, EstablishedTunnel) {
        let local_receive = TunnelId::new(0xC0DE).expect("id");
        let hops = vec![EstablishedHop::with_next(
            peer(0x20),
            EstablishedRole::InboundGateway,
            TunnelId::new(0x20).expect("id"),
            keys(),
            EstablishedNextHop {
                router: peer(0x21),
                tunnel: local_receive,
            },
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Inbound,
            TunnelId::new(0x1000).expect("id"),
            hops,
            0,
            Some((peer(0x20), TunnelId::new(0x20).expect("id"))),
            Some(local_receive),
        )
        .expect("inbound established");
        (local_receive, tunnel)
    }

    #[test]
    fn activate_outbound_once_and_first_hop_persists() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let slot = TunnelSlot::from_raw(7);
        let role = registry
            .activate_outbound(slot, outbound_established(0x1000), 60_000)
            .expect("activate");
        assert!(role.is_usable(0));
        assert_eq!(
            registry.outbound_first_hop(slot),
            Some((peer(0x10).hash(), TunnelId::new(0x1001).expect("id")))
        );
        // Second activation on the same slot fails closed.
        let duplicate = registry.activate_outbound(slot, outbound_established(0x2000), 60_000);
        assert!(matches!(duplicate, Err(RegistryError::DuplicateOutbound(_))));
    }

    #[test]
    fn activate_inbound_once_and_first_hop_persists() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let (local_receive, tunnel) = inbound_established();
        registry
            .activate_inbound(tunnel, 16, 4096, 60_000, 0, 60_000)
            .expect("activate");
        assert_eq!(
            registry.inbound_first_hop(local_receive),
            Some(peer(0x20).hash())
        );
        // Second activation on the same local receive id fails closed.
        let (_id, duplicate_tunnel) = inbound_established();
        let duplicate = registry.activate_inbound(
            duplicate_tunnel,
            16,
            4096,
            60_000,
            0,
            60_000,
        );
        assert!(matches!(duplicate, Err(RegistryError::DuplicateInbound(_))));
    }

    #[test]
    fn outbound_direction_mismatch_is_rejected() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let (_local, inbound_tunnel) = inbound_established();
        let err = registry.activate_outbound(TunnelSlot::from_raw(1), inbound_tunnel, 60_000);
        assert!(matches!(err, Err(RegistryError::DirectionMismatch)));
    }

    #[test]
    fn remove_outbound_drops_first_hop_metadata() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let slot = TunnelSlot::from_raw(2);
        registry
            .activate_outbound(slot, outbound_established(0x3000), 60_000)
            .expect("activate");
        let _ = registry.remove_outbound(slot).expect("role");
        assert!(registry.outbound_first_hop(slot).is_none());
        assert_eq!(registry.outbound_len(), 0);
    }

    #[test]
    fn capacity_overflow_rejects_excess_activation() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(1, 1));
        registry
            .activate_outbound(TunnelSlot::from_raw(1), outbound_established(0x4000), 60_000)
            .expect("first");
        let err = registry.activate_outbound(
            TunnelSlot::from_raw(2),
            outbound_established(0x5000),
            60_000,
        );
        assert!(matches!(err, Err(RegistryError::OutboundFull)));
        let _ = ExploratoryPoolConfig::balanced();
    }
}