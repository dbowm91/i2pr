//! Bounded exploratory tunnel pool.
//!
//! Plan 107 §3.3 owns the deterministic exploratory pool. The pool
//! stores up to the configured number of inbound and outbound tunnel
//! registrations together with their secret-bearing companion
//! material. Each pool entry pairs the public metadata the pool
//! exposes for non-secret inspection with the per-hop `LayerKeys`
//! the data plane needs. The pool does not own a clock; callers
//! must invoke [`ExploratoryPool::advance_time`] to advance the
//! pool's view of time so expiry and replacement decisions stay
//! deterministic.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use i2pr_netdb::{ReplyPath, ReplyPathError, RouterHash};

use crate::config::ExploratoryPoolConfig;
use crate::established::EstablishedMaterial;
use crate::identity::{TunnelDirection, TunnelId, TunnelPeer, TunnelState};

/// The hard ceiling on the number of registered peer hops in a single
/// tunnel.
pub const MAX_HOPS_PER_TUNNEL: u8 = 8;

/// Stable identifier used to address a single tunnel slot in the
/// pool.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TunnelSlot(u32);

impl TunnelSlot {
    /// Returns the inner numeric value.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Constructs a `TunnelSlot` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }
}

impl fmt::Display for TunnelSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Registration record describing a tunnel known to the pool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TunnelRegistration {
    slot: TunnelSlot,
    tunnel_id: TunnelId,
    direction: TunnelDirection,
    state: TunnelState,
    hops: Vec<TunnelPeer>,
    created_at_seconds: u64,
}

impl TunnelRegistration {
    /// Constructs a registration with full validation.
    pub fn new(
        slot: TunnelSlot,
        tunnel_id: TunnelId,
        direction: TunnelDirection,
        state: TunnelState,
        hops: Vec<TunnelPeer>,
        created_at_seconds: u64,
    ) -> Result<Self, RegistrationError> {
        if hops.is_empty() {
            return Err(RegistrationError::EmptyHopList);
        }
        if hops.len() > MAX_HOPS_PER_TUNNEL as usize {
            return Err(RegistrationError::TooManyHops {
                actual: hops.len(),
                maximum: MAX_HOPS_PER_TUNNEL as usize,
            });
        }
        Ok(Self {
            slot,
            tunnel_id,
            direction,
            state,
            hops,
            created_at_seconds,
        })
    }

    /// Returns the slot identifier.
    pub const fn slot(&self) -> TunnelSlot {
        self.slot
    }

    /// Returns the tunnel identifier.
    pub const fn tunnel_id(&self) -> TunnelId {
        self.tunnel_id
    }

    /// Returns the tunnel direction.
    pub const fn direction(&self) -> TunnelDirection {
        self.direction
    }

    /// Returns the tunnel state.
    pub const fn state(&self) -> TunnelState {
        self.state
    }

    /// Returns the hop list.
    pub fn hops(&self) -> &[TunnelPeer] {
        &self.hops
    }

    /// Returns the registration creation timestamp in seconds.
    pub const fn created_at_seconds(&self) -> u64 {
        self.created_at_seconds
    }
}

/// Validation failures for [`TunnelRegistration`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationError {
    /// Hop list was empty.
    EmptyHopList,
    /// Hop list exceeded the documented maximum.
    TooManyHops {
        /// Actual hop count.
        actual: usize,
        /// Maximum accepted hop count.
        maximum: usize,
    },
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyHopList => formatter.write_str("tunnel hop list must not be empty"),
            Self::TooManyHops { actual, maximum } => write!(
                formatter,
                "tunnel hop count {actual} exceeds maximum {maximum}"
            ),
        }
    }
}

impl std::error::Error for RegistrationError {}

/// Outcome of a successful registration attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterOutcome {
    /// The pool accepted the new tunnel as a fresh slot.
    Inserted {
        /// The slot identifier assigned to the new tunnel.
        slot: TunnelSlot,
        /// Whether the new slot displaced an existing established
        /// tunnel in the same direction.
        replaced: Option<TunnelSlot>,
    },
    /// The pool already contains an established tunnel with the same
    /// tunnel identifier; the registration is treated as a no-op.
    Duplicate {
        /// The slot identifier of the existing tunnel.
        slot: TunnelSlot,
    },
}

impl RegisterOutcome {
    /// Returns the assigned slot regardless of the variant.
    pub fn slot(&self) -> TunnelSlot {
        match self {
            Self::Inserted { slot, .. } => *slot,
            Self::Duplicate { slot } => *slot,
        }
    }
}

/// Failure categories for [`ExploratoryPool::register_inbound_with_material`] and
/// [`ExploratoryPool::register_outbound_with_material`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolFullError {
    /// The pool already holds the configured maximum number of
    /// inbound tunnels.
    InboundPoolFull,
    /// The pool already holds the configured maximum number of
    /// outbound tunnels.
    OutboundPoolFull,
}

impl fmt::Display for PoolFullError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::InboundPoolFull => "inbound pool full",
            Self::OutboundPoolFull => "outbound pool full",
        };
        formatter.write_str(label)
    }
}

impl std::error::Error for PoolFullError {}

/// Failure categories for the `register_*` methods that bundle a
/// pool-full failure with the rejected registration record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterError {
    /// The pool is at capacity for the requested direction.
    Full {
        /// Capacity failure category.
        kind: PoolFullError,
        /// The rejected registration.
        registration: TunnelRegistration,
    },
    /// The supplied registration failed validation before the pool
    /// could accept it.
    Invalid(RegistrationError),
}

impl fmt::Display for RegisterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full { kind, .. } => write!(formatter, "{kind}"),
            Self::Invalid(error) => write!(formatter, "invalid registration: {error}"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// Public, non-secret routing metadata the pool exposes for the
/// registered tunnel. The struct survives one-shot secret
/// activation: the data plane registry owns the secret material,
/// while the pool keeps the routing metadata so reply-path
/// selection and outbound first-hop routing continue to work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicTunnelRouting {
    /// First remote hop router hash (for outbound dispatch, this is
    /// the outbound first hop; for inbound tunnels, this is the
    /// remote IBGW).
    first_hop_router: i2pr_proto::Hash,
    /// First remote hop receive tunnel id (for inbound tunnels,
    /// the inbound gateway receive tunnel; for outbound, the
    /// outbound first hop receive tunnel).
    first_hop_receive_tunnel: TunnelId,
    /// Local inbound endpoint receive tunnel id. Only meaningful
    /// for inbound tunnels; outbound entries carry the zero
    /// placeholder.
    local_inbound_receive: TunnelId,
}

impl PublicTunnelRouting {
    /// Constructs public routing metadata from the supplied
    /// established material.
    pub fn from_material(material: &EstablishedMaterial) -> Self {
        let (peer, tunnel) = material.inbound_gateway();
        let first_hop_router = if material.direction() == TunnelDirection::Inbound {
            peer.hash()
        } else if let Some(first) = material.hops().first() {
            first.peer().hash()
        } else {
            i2pr_proto::Hash::from_bytes([0; 32])
        };
        let first_hop_receive_tunnel = if material.direction() == TunnelDirection::Inbound {
            tunnel
        } else if let Some(first) = material.hops().first() {
            first.receive_tunnel()
        } else {
            TunnelId::new(u32::MAX).expect("nonzero")
        };
        Self {
            first_hop_router,
            first_hop_receive_tunnel,
            local_inbound_receive: material.local_inbound_receive(),
        }
    }

    /// Returns the first hop router hash.
    pub const fn first_hop_router(&self) -> i2pr_proto::Hash {
        self.first_hop_router
    }

    /// Returns the first hop receive tunnel id.
    pub const fn first_hop_receive_tunnel(&self) -> TunnelId {
        self.first_hop_receive_tunnel
    }

    /// Returns the local inbound receive tunnel id.
    pub const fn local_inbound_receive(&self) -> TunnelId {
        self.local_inbound_receive
    }
}

/// State of the secret-bearing material bound to one pool entry.
/// The pool keeps the entry and its public routing metadata after
/// activation; the runtime role owner takes the secret material
/// exactly once.
#[derive(Debug)]
pub enum MaterialState {
    /// Secret material is available and has not been activated.
    Available(EstablishedMaterial),
    /// Secret material has been transferred to the runtime role
    /// owner. A second activation attempt returns
    /// [`ActivationError::AlreadyActivated`].
    Activated,
}

impl MaterialState {
    /// Returns whether the material is still available for
    /// activation.
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Available(_))
    }

    /// Returns whether the material has been activated.
    pub const fn is_activated(&self) -> bool {
        matches!(self, Self::Activated)
    }
}

/// Pool-bound entry that pairs the public `TunnelRegistration`
/// metadata and `PublicTunnelRouting` with the secret-bearing
/// `EstablishedMaterial` the data plane consumes. Each pool slot
/// owns exactly one entry. After activation, the entry remains in
/// the pool while the secret material is moved to the runtime role
/// owner.
#[derive(Debug)]
pub struct TunnelEntry {
    registration: TunnelRegistration,
    routing: PublicTunnelRouting,
    material: MaterialState,
}

impl TunnelEntry {
    /// Constructs a new entry pairing a registration with the
    /// established material that came out of the build state
    /// machine.
    pub fn new(registration: TunnelRegistration, established: EstablishedMaterial) -> Self {
        let routing = PublicTunnelRouting::from_material(&established);
        Self {
            registration,
            routing,
            material: MaterialState::Available(established),
        }
    }

    /// Returns the registration metadata.
    pub fn registration(&self) -> &TunnelRegistration {
        &self.registration
    }

    /// Returns the public routing metadata.
    pub const fn routing(&self) -> &PublicTunnelRouting {
        &self.routing
    }

    /// Returns the material state.
    pub const fn material_state(&self) -> &MaterialState {
        &self.material
    }

    /// Returns the established material when it has not yet been
    /// activated. Returns `None` when the material is already in
    /// the activated state.
    pub fn available_material(&self) -> Option<&EstablishedMaterial> {
        match &self.material {
            MaterialState::Available(material) => Some(material),
            MaterialState::Activated => None,
        }
    }
}

/// Bounded exploratory tunnel pool.
///
/// The pool holds up to `max_inbound` inbound and `max_outbound`
/// outbound tunnels. Each slot is keyed by `TunnelSlot` (monotonic).
/// The pool does not own a clock; callers must invoke
/// [`ExploratoryPool::advance_time`] to advance the pool's view of time so
/// expiry and replacement decisions stay deterministic.
#[derive(Debug)]
pub struct ExploratoryPool {
    config: ExploratoryPoolConfig,
    inbound: BTreeMap<TunnelSlot, TunnelEntry>,
    outbound: BTreeMap<TunnelSlot, TunnelEntry>,
    next_slot: u32,
    consecutive_failures: u16,
    paused: bool,
}

impl ExploratoryPool {
    /// Constructs a pool with the supplied bounded configuration.
    pub const fn new(config: ExploratoryPoolConfig) -> Self {
        Self {
            config,
            inbound: BTreeMap::new(),
            outbound: BTreeMap::new(),
            next_slot: 0,
            consecutive_failures: 0,
            paused: false,
        }
    }

    /// Returns the pool configuration.
    pub const fn config(&self) -> ExploratoryPoolConfig {
        self.config
    }

    /// Returns the number of inbound tunnel slots.
    pub fn inbound_len(&self) -> usize {
        self.inbound.len()
    }

    /// Returns the number of outbound tunnel slots.
    pub fn outbound_len(&self) -> usize {
        self.outbound.len()
    }

    /// Returns the number of consecutive failed builds observed by
    /// the pool since the last successful build.
    pub const fn consecutive_failures(&self) -> u16 {
        self.consecutive_failures
    }

    /// Returns whether the pool has paused new builds because the
    /// consecutive failure threshold was reached.
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Returns the current inbound registrations in insertion order.
    pub fn inbound_registrations(&self) -> Vec<TunnelRegistration> {
        self.inbound
            .values()
            .map(|entry| entry.registration.clone())
            .collect()
    }

    /// Returns the current outbound registrations in insertion order.
    pub fn outbound_registrations(&self) -> Vec<TunnelRegistration> {
        self.outbound
            .values()
            .map(|entry| entry.registration.clone())
            .collect()
    }

    /// Inserts an inbound registration built from the supplied
    /// established material. The function performs full capacity
    /// and duplicate checks; on failure the established material is
    /// dropped and zeroized by [`EstablishedMaterial`]'s `Drop`
    /// impl.
    pub fn register_inbound_with_material(
        &mut self,
        established: EstablishedMaterial,
        now_seconds: u64,
    ) -> Result<RegisterOutcome, RegisterError> {
        let tunnel_id = established.creator_tunnel_id();
        let hop_peers: Vec<TunnelPeer> = established.hops().iter().map(|hop| hop.peer()).collect();
        let slot = self.next_slot();
        let registration = match TunnelRegistration::new(
            slot,
            tunnel_id,
            TunnelDirection::Inbound,
            TunnelState::Established,
            hop_peers,
            now_seconds,
        ) {
            Ok(value) => value,
            Err(error) => return Err(RegisterError::Invalid(error)),
        };
        if let Some(existing) = self.find_by_tunnel_id(tunnel_id) {
            return Ok(RegisterOutcome::Duplicate { slot: existing });
        }
        let established_count = self
            .inbound
            .values()
            .filter(|entry| entry.registration().state() == TunnelState::Established)
            .count();
        if established_count >= self.config.max_inbound() as usize {
            return Err(RegisterError::Full {
                kind: PoolFullError::InboundPoolFull,
                registration,
            });
        }
        self.inbound
            .insert(slot, TunnelEntry::new(registration.clone(), established));
        Ok(RegisterOutcome::Inserted {
            slot,
            replaced: None,
        })
    }

    /// Inserts an outbound registration built from the supplied
    /// established material.
    pub fn register_outbound_with_material(
        &mut self,
        established: EstablishedMaterial,
        now_seconds: u64,
    ) -> Result<RegisterOutcome, RegisterError> {
        let tunnel_id = established.creator_tunnel_id();
        let hop_peers: Vec<TunnelPeer> = established.hops().iter().map(|hop| hop.peer()).collect();
        let slot = self.next_slot();
        let registration = match TunnelRegistration::new(
            slot,
            tunnel_id,
            TunnelDirection::Outbound,
            TunnelState::Established,
            hop_peers,
            now_seconds,
        ) {
            Ok(value) => value,
            Err(error) => return Err(RegisterError::Invalid(error)),
        };
        if let Some(existing) = self.find_by_tunnel_id(tunnel_id) {
            return Ok(RegisterOutcome::Duplicate { slot: existing });
        }
        let established_count = self
            .outbound
            .values()
            .filter(|entry| entry.registration().state() == TunnelState::Established)
            .count();
        if established_count >= self.config.max_outbound() as usize {
            return Err(RegisterError::Full {
                kind: PoolFullError::OutboundPoolFull,
                registration,
            });
        }
        self.outbound
            .insert(slot, TunnelEntry::new(registration.clone(), established));
        Ok(RegisterOutcome::Inserted {
            slot,
            replaced: None,
        })
    }

    /// Looks up the established material for the supplied slot.
    /// Returns `None` when the slot has already been activated
    /// (the secret material has moved to the runtime role owner)
    /// or when the slot is unregistered.
    pub fn established(&self, slot: TunnelSlot) -> Option<&EstablishedMaterial> {
        self.inbound
            .get(&slot)
            .or_else(|| self.outbound.get(&slot))
            .and_then(|entry| entry.available_material())
    }

    /// Looks up the registration for the supplied slot.
    pub fn registration(&self, slot: TunnelSlot) -> Option<&TunnelRegistration> {
        self.inbound
            .get(&slot)
            .or_else(|| self.outbound.get(&slot))
            .map(|entry| entry.registration())
    }

    /// Looks up the public routing metadata for the supplied slot.
    /// Returns `None` when the slot is unregistered.
    pub fn routing(&self, slot: TunnelSlot) -> Option<&PublicTunnelRouting> {
        self.inbound
            .get(&slot)
            .or_else(|| self.outbound.get(&slot))
            .map(|entry| entry.routing())
    }

    /// Inserts an inbound registration only (testing seam used by
    /// existing pool tests that do not yet exercise established
    /// material). The function is `#[cfg(test)]` because the pool
    /// must never produce a fake established entry outside tests.
    #[cfg(test)]
    pub fn register_inbound(
        &mut self,
        tunnel_id: TunnelId,
        hops: Vec<TunnelPeer>,
        now_seconds: u64,
    ) -> Result<RegisterOutcome, RegisterError> {
        let slot = self.next_slot();
        let registration = match TunnelRegistration::new(
            slot,
            tunnel_id,
            TunnelDirection::Inbound,
            TunnelState::Established,
            hops,
            now_seconds,
        ) {
            Ok(value) => value,
            Err(error) => return Err(RegisterError::Invalid(error)),
        };
        if let Some(existing) = self.find_by_tunnel_id(tunnel_id) {
            return Ok(RegisterOutcome::Duplicate { slot: existing });
        }
        let established_count = self
            .inbound
            .values()
            .filter(|entry| entry.registration().state() == TunnelState::Established)
            .count();
        if established_count >= self.config.max_inbound() as usize {
            return Err(RegisterError::Full {
                kind: PoolFullError::InboundPoolFull,
                registration,
            });
        }
        // Build a placeholder `EstablishedMaterial` to satisfy the
        // pool's storage invariant.
        let placeholder = build_placeholder_established(TunnelDirection::Inbound, tunnel_id);
        self.inbound
            .insert(slot, TunnelEntry::new(registration.clone(), placeholder));
        Ok(RegisterOutcome::Inserted {
            slot,
            replaced: None,
        })
    }

    /// Inserts an outbound registration only (testing seam).
    #[cfg(test)]
    pub fn register_outbound(
        &mut self,
        tunnel_id: TunnelId,
        hops: Vec<TunnelPeer>,
        now_seconds: u64,
    ) -> Result<RegisterOutcome, RegisterError> {
        let slot = self.next_slot();
        let registration = match TunnelRegistration::new(
            slot,
            tunnel_id,
            TunnelDirection::Outbound,
            TunnelState::Established,
            hops,
            now_seconds,
        ) {
            Ok(value) => value,
            Err(error) => return Err(RegisterError::Invalid(error)),
        };
        if let Some(existing) = self.find_by_tunnel_id(tunnel_id) {
            return Ok(RegisterOutcome::Duplicate { slot: existing });
        }
        let established_count = self
            .outbound
            .values()
            .filter(|entry| entry.registration().state() == TunnelState::Established)
            .count();
        if established_count >= self.config.max_outbound() as usize {
            return Err(RegisterError::Full {
                kind: PoolFullError::OutboundPoolFull,
                registration,
            });
        }
        let placeholder = build_placeholder_established(TunnelDirection::Outbound, tunnel_id);
        self.outbound
            .insert(slot, TunnelEntry::new(registration.clone(), placeholder));
        Ok(RegisterOutcome::Inserted {
            slot,
            replaced: None,
        })
    }

    /// Advances the pool's view of time and returns the slots that
    /// transitioned to [`TunnelState::Expired`].
    pub fn advance_time(&mut self, now_seconds: u64) -> Vec<TunnelSlot> {
        let lifetime = self.config.lifetime().seconds() as u64;
        let mut evicted = Vec::new();
        let mut to_remove: Vec<TunnelSlot> = Vec::new();
        let inbound_iter = self
            .inbound
            .iter_mut()
            .map(|(slot, entry)| (*slot, entry))
            .collect::<Vec<_>>();
        for (slot, entry) in inbound_iter {
            if entry.registration.state() != TunnelState::Established {
                continue;
            }
            if now_seconds.saturating_sub(entry.registration.created_at_seconds()) >= lifetime {
                entry.registration.state = TunnelState::Expired;
                evicted.push(slot);
                to_remove.push(slot);
            }
        }
        let outbound_iter = self
            .outbound
            .iter_mut()
            .map(|(slot, entry)| (*slot, entry))
            .collect::<Vec<_>>();
        for (slot, entry) in outbound_iter {
            if entry.registration.state() != TunnelState::Established {
                continue;
            }
            if now_seconds.saturating_sub(entry.registration.created_at_seconds()) >= lifetime {
                entry.registration.state = TunnelState::Expired;
                evicted.push(slot);
                to_remove.push(slot);
            }
        }
        for slot in to_remove {
            let _ = self.remove(slot);
        }
        evicted
    }

    /// Marks the supplied slot as failed and removes it from the
    /// pool. Returns the removed registration when present.
    pub fn mark_failed(&mut self, slot: TunnelSlot) -> Option<TunnelRegistration> {
        let removed = self.remove(slot);
        if removed.is_some() {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
            self.paused = self.consecutive_failures >= self.config.failure_threshold();
        }
        removed.map(|entry| entry.registration)
    }

    /// Marks the supplied slot as established (used when a build
    /// completes after registration). Resets the failure counter.
    pub fn mark_established(&mut self, slot: TunnelSlot) -> Result<(), PoolError> {
        let entry = self
            .inbound
            .get_mut(&slot)
            .or_else(|| self.outbound.get_mut(&slot))
            .ok_or(PoolError::UnknownSlot(slot))?;
        entry.registration.state = TunnelState::Established;
        self.consecutive_failures = 0;
        self.paused = false;
        Ok(())
    }

    /// Removes the supplied slot unconditionally. Returns the removed
    /// entry (containing the established material) when present.
    pub fn remove(&mut self, slot: TunnelSlot) -> Option<TunnelEntry> {
        self.inbound
            .remove(&slot)
            .or_else(|| self.outbound.remove(&slot))
    }

    /// One-shot activation seam that transfers the
    /// [`crate::established::EstablishedMaterial`] out of the pool
    /// and returns a typed [`crate::established::EstablishedTunnel`]
    /// the data-plane role owner can consume. The pool entry is
    /// **preserved**: the registration and the public routing
    /// metadata remain in the pool so reply-path selection and
    /// capacity accounting continue to work. The material state
    /// transitions to [`MaterialState::Activated`] so a second
    /// activation attempt fails closed with
    /// [`ActivationError::AlreadyActivated`].
    pub fn activate(
        &mut self,
        slot: TunnelSlot,
    ) -> Result<crate::established::EstablishedTunnel, ActivationError> {
        let entry = self
            .inbound
            .get_mut(&slot)
            .or_else(|| self.outbound.get_mut(&slot))
            .ok_or(ActivationError::UnknownSlot(slot))?;
        let material = match std::mem::replace(&mut entry.material, MaterialState::Activated) {
            MaterialState::Available(material) => material,
            MaterialState::Activated => return Err(ActivationError::AlreadyActivated),
        };
        if !material.is_extracted() {
            // Restore the material state and reject placeholder
            // entries; pool integrity forbids second mutations.
            entry.material = MaterialState::Activated;
            return Err(ActivationError::PlaceholderMaterial);
        }
        let mut material = material;
        material
            .into_established_tunnel()
            .ok_or(ActivationError::PlaceholderMaterial)
    }

    /// Selects one inbound tunnel suitable to serve as a reply
    /// path. The selector prefers the oldest established inbound
    /// tunnel whose `created_at_seconds` value is still inside the
    /// configured lifetime window. Expired and failed tunnels are
    /// never returned.
    ///
    /// The selector returns the **first remote IBGW router hash
    /// and its receive tunnel id**; subsequent hops are reachable
    /// through the registered public routing metadata. The selector
    /// continues to work after the secret material has been
    /// activated into the runtime role owner because the public
    /// routing metadata is retained in the pool.
    pub fn select_inbound_reply_path(
        &self,
        now_seconds: u64,
    ) -> Option<Result<ReplyPath, ReplyPathError>> {
        let lifetime = self.config.lifetime().seconds() as u64;
        let mut best: Option<&TunnelEntry> = None;
        for entry in self.inbound.values() {
            let registration = entry.registration();
            if registration.state() != TunnelState::Established {
                continue;
            }
            if now_seconds.saturating_sub(registration.created_at_seconds()) >= lifetime {
                continue;
            }
            if best.is_none()
                || registration.created_at_seconds()
                    < best.expect("set above").registration().created_at_seconds()
            {
                best = Some(entry);
            }
        }
        let chosen = best?;
        let routing = chosen.routing();
        let router = RouterHash::from_hash(routing.first_hop_router());
        Some(ReplyPath::new(
            router,
            routing.first_hop_receive_tunnel().get(),
        ))
    }

    fn find_by_tunnel_id(&self, tunnel_id: TunnelId) -> Option<TunnelSlot> {
        self.inbound
            .iter()
            .chain(self.outbound.iter())
            .find_map(|(slot, entry)| {
                if entry.registration().tunnel_id() == tunnel_id {
                    Some(*slot)
                } else {
                    None
                }
            })
    }

    fn next_slot(&mut self) -> TunnelSlot {
        let slot = TunnelSlot(self.next_slot);
        self.next_slot = self.next_slot.saturating_add(1);
        slot
    }
}

/// Build a synthetic `EstablishedMaterial` placeholder for pool
/// entry insertions that do not yet exercise established material.
/// The placeholder carries a single zero-hop hop vector (which the
/// registration hop list mirrors) and is zeroized on drop. This
/// helper is `#[cfg(test)]` because production code must never
/// produce a fake established entry.
#[cfg(test)]
fn build_placeholder_established(
    direction: TunnelDirection,
    tunnel_id: TunnelId,
) -> EstablishedMaterial {
    use crate::build_crypto::LayerKeys;
    use crate::established::{
        EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole,
    };
    let hop = match direction {
        TunnelDirection::Inbound => EstablishedHop::with_next(
            TunnelPeer::from_hash(i2pr_proto::Hash::from_bytes([0; 32])),
            EstablishedRole::InboundGateway,
            tunnel_id,
            LayerKeys::new([0; 32], [1; 32], [2; 32]),
            EstablishedNextHop::new(
                TunnelPeer::from_hash(i2pr_proto::Hash::from_bytes([0; 32])),
                tunnel_id,
            ),
        ),
        TunnelDirection::Outbound => EstablishedHop::terminal(
            TunnelPeer::from_hash(i2pr_proto::Hash::from_bytes([0; 32])),
            EstablishedRole::OutboundEndpoint,
            tunnel_id,
            LayerKeys::new([0; 32], [1; 32], [2; 32]),
        ),
    };
    let inbound_gateway = if matches!(direction, TunnelDirection::Inbound) {
        Some((hop.peer(), tunnel_id))
    } else {
        None
    };
    let local_inbound_receive = match direction {
        TunnelDirection::Inbound => Some(tunnel_id),
        TunnelDirection::Outbound => None,
    };
    EstablishedMaterial {
        direction,
        creator_tunnel_id: tunnel_id,
        hops: vec![hop],
        created_at_seconds: 0,
        inbound_gateway: inbound_gateway.unwrap_or((
            TunnelPeer::from_hash(i2pr_proto::Hash::from_bytes([0; 32])),
            tunnel_id,
        )),
        local_inbound_receive: local_inbound_receive.unwrap_or(tunnel_id),
        extracted: true,
    }
}

/// Pool-level errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolError {
    /// The supplied slot is not registered.
    UnknownSlot(TunnelSlot),
}

impl fmt::Display for PoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSlot(slot) => write!(formatter, "unknown tunnel slot {slot}"),
        }
    }
}

impl std::error::Error for PoolError {}

/// Failure categories for [`ExploratoryPool::activate`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivationError {
    /// The supplied slot is not registered.
    UnknownSlot(TunnelSlot),
    /// The slot has already been activated.
    AlreadyActivated,
    /// The established material did not carry real per-hop layer
    /// keys (test-only placeholder material cannot enter a
    /// production activation path).
    PlaceholderMaterial,
}

impl fmt::Display for ActivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSlot(slot) => write!(formatter, "unknown tunnel slot {slot}"),
            Self::AlreadyActivated => {
                formatter.write_str("tunnel slot has already been activated")
            }
            Self::PlaceholderMaterial => formatter.write_str(
                "tunnel slot carries placeholder material that cannot enter a production activation",
            ),
        }
    }
}

impl std::error::Error for ActivationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ExploratoryPoolConfig;
    use crate::data_plane_registry::{DataPlaneCapacity, DataPlaneRegistry};

    fn peer(value: u8) -> TunnelPeer {
        TunnelPeer::from_hash(i2pr_proto::Hash::from_bytes([value; 32]))
    }

    #[test]
    fn register_inbound_under_capacity_succeeds() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let tunnel_id = TunnelId::new(0x1000).expect("nonzero");
        let outcome = pool
            .register_inbound(tunnel_id, vec![peer(1), peer(2)], 0)
            .expect("insert");
        assert!(matches!(outcome, RegisterOutcome::Inserted { .. }));
        assert_eq!(pool.inbound_len(), 1);
    }

    #[test]
    fn register_inbound_at_capacity_returns_pool_full() {
        let config = ExploratoryPoolConfig::try_new(1, 1, 2, 600, 1, 4).expect("config");
        let mut pool = ExploratoryPool::new(config);
        let tunnel_id_a = TunnelId::new(0x1000).expect("nonzero");
        let tunnel_id_b = TunnelId::new(0x2000).expect("nonzero");
        pool.register_inbound(tunnel_id_a, vec![peer(1)], 0)
            .expect("insert");
        let error = pool
            .register_inbound(tunnel_id_b, vec![peer(2)], 0)
            .unwrap_err();
        match error {
            RegisterError::Full { kind, .. } => {
                assert_eq!(kind, PoolFullError::InboundPoolFull);
            }
            other => panic!("expected pool full, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_tunnel_id_is_a_no_op() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let tunnel_id = TunnelId::new(0x1000).expect("nonzero");
        pool.register_inbound(tunnel_id, vec![peer(1)], 0)
            .expect("insert");
        let outcome = pool
            .register_inbound(tunnel_id, vec![peer(1)], 0)
            .expect("deduped");
        assert!(matches!(outcome, RegisterOutcome::Duplicate { .. }));
        assert_eq!(pool.inbound_len(), 1);
    }

    #[test]
    fn advance_time_expires_outside_lifetime_window() {
        let config = ExploratoryPoolConfig::try_new(2, 2, 2, 60, 1, 4).expect("config");
        let mut pool = ExploratoryPool::new(config);
        let tunnel_id = TunnelId::new(0x1000).expect("nonzero");
        let slot = match pool
            .register_inbound(tunnel_id, vec![peer(1)], 0)
            .expect("insert")
        {
            RegisterOutcome::Inserted { slot, .. } => slot,
            _ => unreachable!("insert expected"),
        };
        let evicted = pool.advance_time(60);
        assert_eq!(evicted, vec![slot]);
        assert!(
            !pool
                .select_inbound_reply_path(60)
                .map(|inner| inner.is_ok())
                .unwrap_or(false)
        );
    }

    #[test]
    fn select_inbound_reply_path_returns_oldest_active_tunnel() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let id_a = TunnelId::new(0x1000).expect("nonzero");
        let id_b = TunnelId::new(0x2000).expect("nonzero");
        pool.register_inbound(id_a, vec![peer(1)], 0).expect("a");
        pool.register_inbound(id_b, vec![peer(2)], 5).expect("b");
        let path = pool
            .select_inbound_reply_path(5)
            .expect("path")
            .expect("ok");
        assert_eq!(path.tunnel_id(), 0x1000);
    }

    #[test]
    fn mark_failed_increments_failure_counter() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let id = TunnelId::new(0x1000).expect("nonzero");
        let slot = match pool.register_inbound(id, vec![peer(1)], 0).expect("insert") {
            RegisterOutcome::Inserted { slot, .. } => slot,
            _ => unreachable!(),
        };
        assert_eq!(pool.consecutive_failures(), 0);
        pool.mark_failed(slot).expect("removed");
        assert_eq!(pool.consecutive_failures(), 1);
        assert!(!pool.is_paused());
    }

    #[test]
    fn pool_pauses_after_threshold_failures() {
        let config = ExploratoryPoolConfig::try_new(2, 2, 2, 600, 1, 2).expect("config");
        let mut pool = ExploratoryPool::new(config);
        let id_a = TunnelId::new(0x1000).expect("nonzero");
        let id_b = TunnelId::new(0x2000).expect("nonzero");
        let id_c = TunnelId::new(0x3000).expect("nonzero");
        let slot_a = match pool
            .register_inbound(id_a, vec![peer(1)], 0)
            .expect("insert")
        {
            RegisterOutcome::Inserted { slot, .. } => slot,
            _ => unreachable!(),
        };
        let slot_b = match pool
            .register_inbound(id_b, vec![peer(2)], 0)
            .expect("insert")
        {
            RegisterOutcome::Inserted { slot, .. } => slot,
            _ => unreachable!(),
        };
        pool.register_inbound(id_c, vec![peer(3)], 0)
            .expect_err("full");
        pool.mark_failed(slot_a).expect("removed");
        pool.mark_failed(slot_b).expect("removed");
        assert_eq!(pool.consecutive_failures(), 2);
        assert!(pool.is_paused());
    }

    #[test]
    fn mark_established_resets_failure_counter() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let id = TunnelId::new(0x1000).expect("nonzero");
        let slot = match pool.register_inbound(id, vec![peer(1)], 0).expect("insert") {
            RegisterOutcome::Inserted { slot, .. } => slot,
            _ => unreachable!(),
        };
        pool.mark_failed(slot).expect("removed");
        assert_eq!(pool.consecutive_failures(), 1);
        let id2 = TunnelId::new(0x2000).expect("nonzero");
        let slot2 = match pool
            .register_inbound(id2, vec![peer(2)], 0)
            .expect("insert")
        {
            RegisterOutcome::Inserted { slot, .. } => slot,
            _ => unreachable!(),
        };
        pool.mark_established(slot2).expect("ok");
        assert_eq!(pool.consecutive_failures(), 0);
        assert!(!pool.is_paused());
    }

    #[test]
    fn empty_hop_list_is_rejected() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let id = TunnelId::new(0x1000).expect("nonzero");
        let error = pool.register_inbound(id, Vec::new(), 0).unwrap_err();
        match error {
            RegisterError::Invalid(RegistrationError::EmptyHopList) => {}
            other => panic!("expected empty hop list, got {other:?}"),
        }
        assert_eq!(pool.inbound_len(), 0);
    }

    // ---- Plan 117 corrective closure Phase C3 regression matrix ----

    fn real_inbound_material(creator: u32, local_receive: u32) -> EstablishedMaterial {
        use crate::build_crypto::LayerKeys;
        use crate::established::{
            EstablishedHop, EstablishedMaterial, EstablishedNextHop, EstablishedRole,
        };
        let creator_id = TunnelId::new(creator).expect("id");
        let local = TunnelId::new(local_receive).expect("id");
        let ibgw_tunnel = TunnelId::new(creator + 0x10).expect("id");
        let hop = EstablishedHop::with_next(
            peer(0x20),
            EstablishedRole::InboundGateway,
            ibgw_tunnel,
            LayerKeys::new([0; 32], [1; 32], [2; 32]),
            EstablishedNextHop::new(peer(0x21), local),
        );
        EstablishedMaterial {
            direction: TunnelDirection::Inbound,
            creator_tunnel_id: creator_id,
            hops: vec![hop],
            created_at_seconds: 0,
            inbound_gateway: (peer(0x20), ibgw_tunnel),
            local_inbound_receive: local,
            extracted: true,
        }
    }

    fn register_real_inbound(
        pool: &mut ExploratoryPool,
        creator: u32,
        local_receive: u32,
    ) -> TunnelSlot {
        let material = real_inbound_material(creator, local_receive);
        let outcome = pool
            .register_inbound_with_material(material, 0)
            .expect("register");
        outcome.slot()
    }

    #[test]
    fn activate_preserves_inbound_reply_path() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let slot = register_real_inbound(&mut pool, 0x1000, 0x901);
        let path_before = pool
            .select_inbound_reply_path(0)
            .expect("path present")
            .expect("ok");
        pool.activate(slot).expect("activate");
        let path_after = pool
            .select_inbound_reply_path(0)
            .expect("path present")
            .expect("ok");
        assert_eq!(path_before.tunnel_id(), path_after.tunnel_id());
        assert_eq!(
            path_before.gateway().as_hash(),
            path_after.gateway().as_hash()
        );
        // Reply-path selection must remain after activation.
        let _ = path_after;
    }

    #[test]
    fn activate_preserves_registration_count() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let slot = register_real_inbound(&mut pool, 0x1100, 0x902);
        assert_eq!(pool.inbound_len(), 1);
        pool.activate(slot).expect("activate");
        assert_eq!(pool.inbound_len(), 1);
    }

    #[test]
    fn activate_preserves_duplicate_identity() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let slot = register_real_inbound(&mut pool, 0x1200, 0x903);
        pool.activate(slot).expect("activate");
        // A duplicate registration (same tunnel id) must still be
        // treated as a duplicate rather than a new slot.
        let material = real_inbound_material(0x1200, 0x903);
        let outcome = pool
            .register_inbound_with_material(material, 0)
            .expect("duplicate deduped");
        assert!(matches!(outcome, RegisterOutcome::Duplicate { .. }));
    }

    #[test]
    fn second_activation_returns_already_activated() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let slot = register_real_inbound(&mut pool, 0x1300, 0x904);
        pool.activate(slot).expect("first");
        let error = pool.activate(slot).expect_err("second must fail closed");
        assert_eq!(error, ActivationError::AlreadyActivated);
    }

    #[test]
    fn activated_entry_expires_normally() {
        let config = ExploratoryPoolConfig::try_new(2, 2, 2, 60, 1, 4).expect("config");
        let mut pool = ExploratoryPool::new(config);
        let slot = register_real_inbound(&mut pool, 0x1400, 0x905);
        pool.activate(slot).expect("activate");
        let evicted = pool.advance_time(60);
        assert_eq!(evicted, vec![slot]);
        assert_eq!(pool.inbound_len(), 0);
    }

    #[test]
    fn activated_entry_mark_failed_removes_registration() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let slot = register_real_inbound(&mut pool, 0x1500, 0x906);
        pool.activate(slot).expect("activate");
        let removed = pool.mark_failed(slot).expect("removed");
        assert_eq!(removed.slot(), slot);
        assert_eq!(pool.inbound_len(), 0);
    }

    #[test]
    fn pool_eviction_can_remove_matching_registry_role() {
        // The pool and the registry cooperate: when the pool
        // evicts an entry, the registry can drop the corresponding
        // role by slot.
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let slot = register_real_inbound(&mut pool, 0x1600, 0x907);
        let activated = pool.activate(slot).expect("activate");
        let local_receive = activated.local_inbound_receive();
        let mut registry = DataPlaneRegistry::new(DataPlaneCapacity::new(4, 4));
        registry
            .activate_inbound(activated, 16, 4096, 60_000, 0, 60_000)
            .expect("registry activate");
        assert_eq!(registry.inbound_len(), 1);
        let _ = pool.mark_failed(slot);
        let removed = registry.remove_inbound(local_receive);
        assert!(removed.is_some());
        assert_eq!(registry.inbound_len(), 0);
    }

    #[test]
    fn activation_transfers_secret_material_once() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let slot = register_real_inbound(&mut pool, 0x1700, 0x908);
        // First activation succeeds and the entry remains with
        // MaterialState::Activated.
        let tunnel = pool.activate(slot).expect("activate");
        assert_eq!(tunnel.direction(), TunnelDirection::Inbound);
        let entry = pool.routing(slot).expect("routing preserved");
        assert_eq!(entry.first_hop_router(), peer(0x20).hash());
    }

    #[test]
    fn unknown_slot_activation_returns_unknown_slot() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let error = pool
            .activate(TunnelSlot::from_raw(0xFFFF))
            .expect_err("must reject");
        assert_eq!(
            error,
            ActivationError::UnknownSlot(TunnelSlot::from_raw(0xFFFF))
        );
    }

    #[test]
    fn public_routing_metadata_is_derived_from_established_material() {
        let mut pool = ExploratoryPool::new(ExploratoryPoolConfig::balanced());
        let slot = register_real_inbound(&mut pool, 0x1800, 0x909);
        let routing = pool.routing(slot).expect("routing");
        assert_eq!(routing.first_hop_router(), peer(0x20).hash());
        assert_eq!(routing.first_hop_receive_tunnel().get(), 0x1810);
        assert_eq!(routing.local_inbound_receive().get(), 0x909);
    }
}
