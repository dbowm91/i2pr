//! Plan 117 bounded runtime-side data-plane role registry.
//!
//! The registry is the explicit owner of activated local roles:
//!
//! - one outbound slot per [`TunnelSlot`] maps to one
//!   [`OutboundGatewayRole`];
//! - one inbound [`TunnelSlot`] maps to one local receive tunnel id and
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
    /// The supplied pool slot is already bound to an inbound role.
    DuplicateInboundSlot(TunnelSlot),
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
            Self::DuplicateInboundSlot(slot) => {
                write!(formatter, "inbound slot {slot} is already bound")
            }
            Self::DirectionMismatch => {
                formatter.write_str("tunnel direction does not match registry entry")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// Role removed by a pool-slot lifecycle event.
#[derive(Debug)]
pub enum RegistryRemoval {
    /// An outbound role was removed.
    Outbound(OutboundGatewayRole),
    /// An inbound role and its reverse metadata were removed.
    Inbound(LocalInboundEndpointRole),
    /// The slot was not present in the registry.
    Unknown(TunnelSlot),
}

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
    /// Reverse mapping from the pool's canonical inbound slot to the
    /// local receive tunnel id. Pool expiry/failure reports slots, so
    /// this mapping keeps lifecycle cleanup independent of an
    /// out-of-band receive-id copy.
    inbound_slot_to_receive: BTreeMap<TunnelSlot, TunnelId>,
    /// Reverse mapping used to remove all inbound metadata atomically
    /// when callers still have the local receive id.
    inbound_receive_to_slot: BTreeMap<TunnelId, TunnelSlot>,
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
            inbound_slot_to_receive: BTreeMap::new(),
            inbound_receive_to_slot: BTreeMap::new(),
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
        self.outbound_first_hop
            .insert(slot, (first_hop, receive_tunnel));
        self.outbound.insert(slot, role);
        Ok(self.outbound.get(&slot).expect("just inserted"))
    }

    /// Activates an established inbound tunnel and binds it to
    /// the supplied pool slot and local inbound receive tunnel id.
    /// Returns the role the data plane can consume.
    #[allow(clippy::too_many_arguments)]
    pub fn activate_inbound(
        &mut self,
        slot: TunnelSlot,
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
        if self.inbound_slot_to_receive.contains_key(&slot) || self.outbound.contains_key(&slot) {
            return Err(RegistryError::DuplicateInboundSlot(slot));
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
        self.inbound_slot_to_receive.insert(slot, local_receive);
        self.inbound_receive_to_slot.insert(local_receive, slot);
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

    /// Returns the pool slot bound to the supplied local receive
    /// tunnel id, when the inbound role is registered.
    pub fn inbound_slot(&self, local_receive: TunnelId) -> Option<TunnelSlot> {
        self.inbound_receive_to_slot.get(&local_receive).copied()
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
        if let Some(slot) = self.inbound_receive_to_slot.remove(&local_receive) {
            self.inbound_slot_to_receive.remove(&slot);
        }
        role
    }

    /// Removes the role associated with a pool slot and all of its
    /// public metadata. Pool expiry and failure paths should call this
    /// method with the slots returned by `advance_time` or `mark_failed`.
    /// An unknown slot is returned as a typed bounded outcome.
    pub fn remove_slot(&mut self, slot: TunnelSlot) -> RegistryRemoval {
        if let Some(role) = self.remove_outbound(slot) {
            return RegistryRemoval::Outbound(role);
        }
        if let Some(local_receive) = self.inbound_slot_to_receive.get(&slot).copied() {
            return self
                .remove_inbound(local_receive)
                .map(RegistryRemoval::Inbound)
                .unwrap_or(RegistryRemoval::Unknown(slot));
        }
        RegistryRemoval::Unknown(slot)
    }

    /// Returns whether the registry currently holds at least one
    /// outbound role whose role-level `is_usable` check passes at
    /// the supplied time. The check filters out expired roles and
    /// direction mismatches; an empty registry always returns
    /// `false`.
    pub fn has_usable_outbound_role(&self, now_ms: u64) -> bool {
        self.outbound.values().any(|role| role.is_usable(now_ms))
    }

    /// Returns whether the registry currently holds at least one
    /// inbound endpoint role whose role-level `is_usable` check
    /// passes at the supplied time. The check filters out expired
    /// roles and direction mismatches; an empty registry always
    /// returns `false`.
    pub fn has_usable_inbound_role(&self, now_ms: u64) -> bool {
        self.inbound.values().any(|role| role.is_usable(now_ms))
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

    fn inbound_established_with(
        creator: u32,
        local_receive_value: u32,
    ) -> (TunnelId, EstablishedTunnel) {
        let local_receive = TunnelId::new(local_receive_value).expect("id");
        let ibgw_tunnel = TunnelId::new(creator + 0x20).expect("id");
        let hops = vec![EstablishedHop::with_next(
            peer(0x20),
            EstablishedRole::InboundGateway,
            ibgw_tunnel,
            keys(),
            EstablishedNextHop {
                router: peer(0x21),
                tunnel: local_receive,
            },
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Inbound,
            TunnelId::new(creator).expect("id"),
            hops,
            0,
            Some((peer(0x20), ibgw_tunnel)),
            Some(local_receive),
        )
        .expect("inbound established");
        (local_receive, tunnel)
    }

    fn inbound_established() -> (TunnelId, EstablishedTunnel) {
        inbound_established_with(0x1000, 0xC0DE)
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
        assert!(matches!(
            duplicate,
            Err(RegistryError::DuplicateOutbound(_))
        ));
    }

    #[test]
    fn activate_inbound_once_and_first_hop_persists() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let (local_receive, tunnel) = inbound_established();
        let slot = TunnelSlot::from_raw(1);
        registry
            .activate_inbound(slot, tunnel, 16, 4096, 60_000, 0, 60_000)
            .expect("activate");
        assert_eq!(
            registry.inbound_first_hop(local_receive),
            Some(peer(0x20).hash())
        );
        assert_eq!(registry.inbound_slot(local_receive), Some(slot));
        // Second activation on the same local receive id fails closed.
        let (_id, duplicate_tunnel) = inbound_established_with(0x1001, 0xC0DE);
        let duplicate = registry.activate_inbound(
            TunnelSlot::from_raw(2),
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
    fn inbound_duplicate_slot_is_rejected() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let slot = TunnelSlot::from_raw(9);
        let (_, first) = inbound_established();
        registry
            .activate_inbound(slot, first, 16, 4096, 60_000, 0, 60_000)
            .expect("first activation");
        let (_, second) = inbound_established_with(0x1001, 0xC0DF);
        let error = registry
            .activate_inbound(slot, second, 16, 4096, 60_000, 0, 60_000)
            .expect_err("duplicate slot");
        assert_eq!(error, RegistryError::DuplicateInboundSlot(slot));
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
    fn remove_slot_removes_outbound_role() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let slot = TunnelSlot::from_raw(3);
        registry
            .activate_outbound(slot, outbound_established(0x4000), 60_000)
            .expect("activate");
        assert!(matches!(
            registry.remove_slot(slot),
            RegistryRemoval::Outbound(_)
        ));
        assert_eq!(registry.outbound_len(), 0);
        assert!(registry.outbound_first_hop(slot).is_none());
    }

    #[test]
    fn remove_slot_removes_inbound_role_and_reverse_metadata() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let slot = TunnelSlot::from_raw(4);
        let (local_receive, tunnel) = inbound_established();
        registry
            .activate_inbound(slot, tunnel, 16, 4096, 60_000, 0, 60_000)
            .expect("activate");
        assert!(matches!(
            registry.remove_slot(slot),
            RegistryRemoval::Inbound(_)
        ));
        assert_eq!(registry.inbound_len(), 0);
        assert!(registry.inbound(local_receive).is_none());
        assert!(registry.inbound_first_hop(local_receive).is_none());
        assert!(registry.inbound_slot(local_receive).is_none());
        assert!(matches!(
            registry.remove_slot(slot),
            RegistryRemoval::Unknown(_)
        ));
    }

    #[test]
    fn remove_inbound_clears_reverse_mapping() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        let slot = TunnelSlot::from_raw(5);
        let (local_receive, tunnel) = inbound_established();
        registry
            .activate_inbound(slot, tunnel, 16, 4096, 60_000, 0, 60_000)
            .expect("activate");
        assert!(registry.remove_inbound(local_receive).is_some());
        assert!(registry.inbound_slot(local_receive).is_none());
        assert!(matches!(
            registry.remove_slot(slot),
            RegistryRemoval::Unknown(_)
        ));
    }

    #[test]
    fn unknown_slot_cleanup_is_bounded_and_typed() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(1, 1));
        let slot = TunnelSlot::from_raw(u32::MAX);
        assert!(matches!(
            registry.remove_slot(slot),
            RegistryRemoval::Unknown(unknown) if unknown == slot
        ));
    }

    #[test]
    fn capacity_overflow_rejects_excess_activation() {
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(1, 1));
        registry
            .activate_outbound(
                TunnelSlot::from_raw(1),
                outbound_established(0x4000),
                60_000,
            )
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
