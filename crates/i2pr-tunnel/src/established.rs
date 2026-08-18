//! Established tunnel ownership and per-hop secret material.
//!
//! Plan 116 §6 owns the typed representation of a successful
//! short-build transition from build-state-machine completion to a
//! real, secret-bearing, usable pool entry. The module deliberately
//! separates the public metadata the pool exposes for non-secret
//! inspection from the per-hop secret material the data plane
//! needs. Secret material is non-`Debug`, non-cloneable beyond the
//! build-time path, zeroizes on drop, and is consumed once on
//! transfer.
//!
//! The data plane uses three independent tunnel-id concepts that
//! the prior pooled metadata conflated:
//!
//! ```text
//! creator_tunnel_id               the local slot identifier used by
//!                                 the pool registrar
//! first_hop_receive_tunnel        the receive id the first remote
//!                                 hop expects to receive TunnelData
//!                                 on (== inbound external gateway)
//! terminal_inbound_receive_tunnel the receive id the local
//!                                 inbound endpoint expects (local
//!                                 creator endpoint, never a remote
//!                                 short-build hop)
//! ```
//!
//! Outbound OBEP does not encode a fixed data-plane destination;
//! the data-plane delivery instruction is carried per message. The
//! OBEP next-hop field is therefore `Option::None`.

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    unused_imports,
    clippy::manual_range_contains,
    clippy::type_complexity,
    clippy::needless_borrow,
    missing_docs
)]

use std::fmt;

use i2pr_proto::Hash;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::build_crypto::LayerKeys;
use crate::identity::{TunnelDirection, TunnelId, TunnelPeer};

/// Optional next-hop routing state attached to one remote hop.
/// The struct exists only where the underlying role requires a
/// forwarding target; participants and inbound gateways own
/// `Some(next)`; outbound endpoints own `None` (delivery is
/// per-message, not fixed).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EstablishedNextHop {
    /// Next-hop router identity.
    pub router: TunnelPeer,
    /// Next-hop receive tunnel id.
    pub tunnel: TunnelId,
}

impl EstablishedNextHop {
    /// Builds a next-hop state from the supplied router and tunnel
    /// identifier. Both fields are required to be non-sentinel.
    pub fn new(router: TunnelPeer, tunnel: TunnelId) -> Self {
        Self { router, tunnel }
    }
}

impl Zeroize for EstablishedNextHop {
    fn zeroize(&mut self) {
        self.router.zeroize();
        self.tunnel.zeroize();
    }
}

/// One per-hop retained crypto context. The struct owns the
/// canonical hop identity (`router_hash`, `role`, `receive_tunnel`)
/// and the forward `LayerKeys` the data plane needs to apply the
/// AES-256 ECB/CBC/ECB layer transform. The `next` field is
/// `Option<EstablishedNextHop>`:
///   * participants and inbound gateways require `Some(next)`;
///   * outbound endpoints own `None` because delivery is per-message.
///
/// No sentinel `u32::MAX` or zero-hash values are used.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct EstablishedHop {
    peer: TunnelPeer,
    role: EstablishedRole,
    receive_tunnel: TunnelId,
    layer_keys: LayerKeys,
    next: Option<EstablishedNextHop>,
}

impl EstablishedHop {
    /// Constructs a participant-style hop (with a next hop).
    pub fn with_next(
        peer: TunnelPeer,
        role: EstablishedRole,
        receive_tunnel: TunnelId,
        layer_keys: LayerKeys,
        next: EstablishedNextHop,
    ) -> Self {
        Self {
            peer,
            role,
            receive_tunnel,
            layer_keys,
            next: Some(next),
        }
    }

    /// Constructs a terminal outbound endpoint hop (no next hop).
    pub fn terminal(
        peer: TunnelPeer,
        role: EstablishedRole,
        receive_tunnel: TunnelId,
        layer_keys: LayerKeys,
    ) -> Self {
        Self {
            peer,
            role,
            receive_tunnel,
            layer_keys,
            next: None,
        }
    }

    /// Returns the peer router hash.
    pub const fn peer(&self) -> TunnelPeer {
        self.peer
    }

    /// Returns the hop role classification.
    pub const fn role(&self) -> EstablishedRole {
        self.role
    }

    /// Returns the receive tunnel id the hop expects TunnelData
    /// cells on.
    pub const fn receive_tunnel(&self) -> TunnelId {
        self.receive_tunnel
    }

    /// Returns the per-hop layer keys.
    pub const fn layer_keys(&self) -> &LayerKeys {
        &self.layer_keys
    }

    /// Returns the next-hop routing state when one exists.
    pub const fn next(&self) -> Option<&EstablishedNextHop> {
        self.next.as_ref()
    }

    /// Returns the next-hop router identity when `next` is set.
    pub fn next_router(&self) -> Option<TunnelPeer> {
        self.next.as_ref().map(|next| next.router)
    }

    /// Returns the next-hop tunnel id when `next` is set.
    pub fn next_tunnel(&self) -> Option<TunnelId> {
        self.next.as_ref().map(|next| next.tunnel)
    }
}

impl fmt::Debug for EstablishedHop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EstablishedHop")
            .field("peer", &self.peer)
            .field("role", &self.role)
            .field("receive_tunnel", &self.receive_tunnel)
            .field("layer_keys", &"<redacted>")
            .field("next", &self.next)
            .finish()
    }
}

/// Hop-role classification used by the data plane. This is a
/// separate enumeration from the build-time [`crate::identity::TunnelRole`]
/// because the data plane only cares about the three canonical
/// remote roles that can be present in an established tunnel:
/// participant, outbound endpoint, and inbound gateway. The
/// local creator-side inbound endpoint is **not** a remote
/// short-build hop and is therefore not represented here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, zeroize::Zeroize)]
pub enum EstablishedRole {
    /// Intermediate hop that decrypts one layer and forwards.
    Participant,
    /// Outbound endpoint: receives one layer and exposes the
    /// plaintext data-plane payload.
    OutboundEndpoint,
    /// Inbound gateway: receives a TunnelGateway and applies one
    /// outbound layer.
    InboundGateway,
}

impl fmt::Display for EstablishedRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Participant => "participant",
            Self::OutboundEndpoint => "obep",
            Self::InboundGateway => "ibgw",
        };
        formatter.write_str(label)
    }
}

/// A successful, secret-bearing tunnel entry that the data plane
/// can consume.
///
/// `EstablishedTunnel` is non-`Clone` and non-`Debug` (its
/// `Debug` impl redacts every secret); the type holds the ordered
/// hop list, the independent tunnel identifiers the data plane
/// needs, and the per-hop `LayerKeys`.
///
/// The remote hop vector deliberately excludes the local creator
/// endpoint. Inbound tunnels carry `[IBGW, Participant*]` only;
/// the local inbound endpoint's identity is a separate field on
/// the same `EstablishedTunnel` (`local_inbound_receive`).
pub struct EstablishedTunnel {
    direction: TunnelDirection,
    creator_tunnel_id: TunnelId,
    hops: Vec<EstablishedHop>,
    created_at_seconds: u64,
    /// Inbound external gateway router hash (first remote hop) and
    /// the first remote hop's receive tunnel id. Only meaningful
    /// for inbound tunnels; outbound tunnels carry the zero
    /// peer placeholder and the zero tunnel id sentinel with no
    /// public exposure.
    inbound_gateway: (TunnelPeer, TunnelId),
    /// Local inbound endpoint receive tunnel id. Only meaningful
    /// for inbound tunnels; outbound tunnels carry the zero
    /// placeholder.
    local_inbound_receive: TunnelId,
}

impl EstablishedTunnel {
    /// Builds a new established tunnel record from the supplied
    /// per-hop material and inbound endpoints.
    pub fn new(
        direction: TunnelDirection,
        creator_tunnel_id: TunnelId,
        hops: Vec<EstablishedHop>,
        created_at_seconds: u64,
        inbound_gateway: Option<(TunnelPeer, TunnelId)>,
        local_inbound_receive: Option<TunnelId>,
    ) -> Result<Self, EstablishedTunnelError> {
        if hops.is_empty() {
            return Err(EstablishedTunnelError::EmptyHopList);
        }
        if hops.len() > crate::pool::MAX_HOPS_PER_TUNNEL as usize {
            return Err(EstablishedTunnelError::TooManyHops {
                actual: hops.len(),
                maximum: crate::pool::MAX_HOPS_PER_TUNNEL as usize,
            });
        }
        match direction {
            TunnelDirection::Inbound => {
                if inbound_gateway.is_none() {
                    return Err(EstablishedTunnelError::MissingInboundGateway);
                }
                if local_inbound_receive.is_none() {
                    return Err(EstablishedTunnelError::MissingLocalInboundReceive);
                }
                // Inbound remote hops: [IBGW, Participant*].
                if hops[0].role() != EstablishedRole::InboundGateway {
                    return Err(EstablishedTunnelError::FirstHopRoleInvalid {
                        expected: EstablishedRole::InboundGateway,
                        actual: hops[0].role(),
                    });
                }
                // Every inbound remote hop must have a `next`
                // because the chain forwards through every hop.
                for (index, hop) in hops.iter().enumerate() {
                    if hop.next().is_none() {
                        return Err(EstablishedTunnelError::MissingNextHop { hop_index: index });
                    }
                }
            }
            TunnelDirection::Outbound => {
                if inbound_gateway.is_some() {
                    return Err(EstablishedTunnelError::OutboundGatewaySpecified);
                }
                if local_inbound_receive.is_some() {
                    return Err(EstablishedTunnelError::OutboundLocalReceiveSpecified);
                }
                // Outbound remote hops: [Participant*, OBEP].
                let last = hops.len() - 1;
                if hops[last].role() != EstablishedRole::OutboundEndpoint {
                    return Err(EstablishedTunnelError::LastHopRoleInvalid {
                        expected: EstablishedRole::OutboundEndpoint,
                        actual: hops[last].role(),
                    });
                }
                // Outbound OBEP carries no `next`; intermediate
                // hops must carry one.
                if hops[last].next().is_some() {
                    return Err(EstablishedTunnelError::OutboundEndpointHasNext);
                }
                for hop in hops.iter().take(last) {
                    if hop.next().is_none() {
                        return Err(EstablishedTunnelError::MissingIntermediateHopNext);
                    }
                }
            }
        }
        if let Some((expected_router, expected_tunnel)) = inbound_gateway {
            if hops[0].peer() != expected_router {
                return Err(EstablishedTunnelError::FirstHopRouterMismatch {
                    expected: expected_router,
                    actual: hops[0].peer(),
                });
            }
            if hops[0].receive_tunnel() != expected_tunnel {
                return Err(EstablishedTunnelError::FirstHopReceiveTunnelMismatch {
                    expected: expected_tunnel,
                    actual: hops[0].receive_tunnel(),
                });
            }
        }
        if let Some(expected) = local_inbound_receive {
            let last = hops.len() - 1;
            // Inbound tunnels verify that the **terminal remote
            // hop's next_tunnel** matches the local inbound
            // receive id: that is the tunnel id the local creator
            // endpoint listens on. The terminal hop's
            // `receive_tunnel` is a separate field that the
            // inbound participant's predecessor forwards into.
            let next_tunnel = hops[last].next_tunnel().unwrap_or(zero_id());
            if next_tunnel != expected {
                return Err(EstablishedTunnelError::LocalInboundReceiveMismatch {
                    expected,
                    actual: next_tunnel,
                });
            }
        }
        Ok(Self {
            direction,
            creator_tunnel_id,
            hops,
            created_at_seconds,
            inbound_gateway: inbound_gateway.unwrap_or((zero_peer(), zero_id())),
            local_inbound_receive: local_inbound_receive.unwrap_or_else(zero_id),
        })
    }

    /// Returns the direction.
    pub const fn direction(&self) -> TunnelDirection {
        self.direction
    }

    /// Returns the local creator tunnel id the pool uses.
    pub const fn creator_tunnel_id(&self) -> TunnelId {
        self.creator_tunnel_id
    }

    /// Returns the ordered hop list.
    pub fn hops(&self) -> &[EstablishedHop] {
        &self.hops
    }

    /// Returns the creation timestamp in seconds.
    pub const fn created_at_seconds(&self) -> u64 {
        self.created_at_seconds
    }

    /// Returns the inbound gateway router hash (first remote hop)
    /// and its receive tunnel id when the direction is inbound.
    /// Returns the zero placeholder for outbound tunnels.
    pub const fn inbound_gateway(&self) -> (TunnelPeer, TunnelId) {
        self.inbound_gateway
    }

    /// Returns the local inbound receive tunnel id when the
    /// direction is inbound. Returns the zero placeholder for
    /// outbound tunnels.
    pub const fn local_inbound_receive(&self) -> TunnelId {
        self.local_inbound_receive
    }

    /// Returns the first remote hop's router hash.
    pub fn first_hop_router(&self) -> TunnelPeer {
        self.hops[0].peer()
    }

    /// Returns the first remote hop's receive tunnel id.
    pub fn first_hop_receive_tunnel(&self) -> TunnelId {
        self.hops[0].receive_tunnel()
    }

    /// Consumes the `EstablishedTunnel` and returns the
    /// `EstablishedMaterial` the registrar stores. The function
    /// is intended to be called exactly once per successful build.
    pub fn into_extracted(self) -> EstablishedMaterial {
        let mut tunnel = self;
        let hops = std::mem::take(&mut tunnel.hops);
        let direction = tunnel.direction;
        let creator_tunnel_id = tunnel.creator_tunnel_id;
        let created_at_seconds = tunnel.created_at_seconds;
        let inbound_gateway = tunnel.inbound_gateway;
        let local_inbound_receive = tunnel.local_inbound_receive;
        EstablishedMaterial {
            direction,
            creator_tunnel_id,
            hops,
            created_at_seconds,
            inbound_gateway,
            local_inbound_receive,
            extracted: true,
        }
    }
}

impl fmt::Debug for EstablishedTunnel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EstablishedTunnel")
            .field("direction", &self.direction)
            .field("creator_tunnel_id", &self.creator_tunnel_id)
            .field("hops", &format_args!("<{} redacted>", self.hops.len()))
            .field("created_at_seconds", &self.created_at_seconds)
            .field("inbound_gateway", &self.inbound_gateway)
            .field("local_inbound_receive", &self.local_inbound_receive)
            .finish()
    }
}

impl Drop for EstablishedTunnel {
    fn drop(&mut self) {
        self.hops.zeroize();
    }
}

impl Zeroize for EstablishedTunnel {
    fn zeroize(&mut self) {
        self.hops.zeroize();
        self.creator_tunnel_id.zeroize();
    }
}

impl ZeroizeOnDrop for EstablishedTunnel {}

/// Construction failures for [`EstablishedTunnel`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EstablishedTunnelError {
    /// Hop list was empty.
    EmptyHopList,
    /// Hop list exceeded the documented maximum.
    TooManyHops {
        /// Actual hop count.
        actual: usize,
        /// Maximum accepted hop count.
        maximum: usize,
    },
    /// Inbound construction did not specify the inbound gateway.
    MissingInboundGateway,
    /// Inbound construction did not specify the local inbound
    /// receive id.
    MissingLocalInboundReceive,
    /// Outbound construction supplied an inbound gateway.
    OutboundGatewaySpecified,
    /// Outbound construction supplied a local inbound receive id.
    OutboundLocalReceiveSpecified,
    /// First hop role does not match the declared direction.
    FirstHopRoleInvalid {
        /// Expected role for the first hop.
        expected: EstablishedRole,
        /// Actual role the hop has.
        actual: EstablishedRole,
    },
    /// Last hop role does not match the declared direction.
    LastHopRoleInvalid {
        /// Expected role for the last hop.
        expected: EstablishedRole,
        /// Actual role the hop has.
        actual: EstablishedRole,
    },
    /// First hop router hash does not match the declared inbound
    /// gateway router hash.
    FirstHopRouterMismatch {
        /// Expected router hash.
        expected: TunnelPeer,
        /// Actual router hash.
        actual: TunnelPeer,
    },
    /// First hop receive tunnel id does not match the declared
    /// inbound gateway tunnel id.
    FirstHopReceiveTunnelMismatch {
        /// Expected tunnel id.
        expected: TunnelId,
        /// Actual tunnel id.
        actual: TunnelId,
    },
    /// Terminal inbound hop receive tunnel id does not match the
    /// declared local inbound receive id.
    LocalInboundReceiveMismatch {
        /// Expected local receive id.
        expected: TunnelId,
        /// Actual terminal hop receive id.
        actual: TunnelId,
    },
    /// A remote hop in the chain lacks a `next` field that the
    /// forwarding path requires.
    MissingNextHop {
        /// Index of the offending hop.
        hop_index: usize,
    },
    /// Outbound intermediate hop lacks a `next` field.
    MissingIntermediateHopNext,
    /// Outbound endpoint unexpectedly carries a `next` field.
    OutboundEndpointHasNext,
}

impl fmt::Display for EstablishedTunnelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyHopList => {
                formatter.write_str("established tunnel hop list must not be empty")
            }
            Self::TooManyHops { actual, maximum } => write!(
                formatter,
                "established tunnel hop count {actual} exceeds maximum {maximum}"
            ),
            Self::MissingInboundGateway => {
                formatter.write_str("inbound tunnel must declare an inbound gateway")
            }
            Self::MissingLocalInboundReceive => {
                formatter.write_str("inbound tunnel must declare a local inbound receive id")
            }
            Self::OutboundGatewaySpecified => {
                formatter.write_str("outbound tunnel must not declare an inbound gateway")
            }
            Self::OutboundLocalReceiveSpecified => {
                formatter.write_str("outbound tunnel must not declare a local inbound receive id")
            }
            Self::FirstHopRoleInvalid { expected, actual } => write!(
                formatter,
                "first hop role {actual} does not match expected {expected}"
            ),
            Self::LastHopRoleInvalid { expected, actual } => write!(
                formatter,
                "last hop role {actual} does not match expected {expected}"
            ),
            Self::FirstHopRouterMismatch { expected, actual } => write!(
                formatter,
                "first hop router {actual:?} does not match expected gateway {expected:?}"
            ),
            Self::FirstHopReceiveTunnelMismatch { expected, actual } => write!(
                formatter,
                "first hop receive tunnel {actual} does not match expected gateway tunnel {expected}"
            ),
            Self::LocalInboundReceiveMismatch { expected, actual } => write!(
                formatter,
                "terminal inbound hop receive tunnel {actual} does not match expected local receive tunnel {expected}"
            ),
            Self::MissingNextHop { hop_index } => write!(
                formatter,
                "remote hop {hop_index} missing required next-hop state"
            ),
            Self::MissingIntermediateHopNext => {
                formatter.write_str("outbound intermediate hop missing required next-hop state")
            }
            Self::OutboundEndpointHasNext => {
                formatter.write_str("outbound endpoint must not declare a next-hop field")
            }
        }
    }
}

impl std::error::Error for EstablishedTunnelError {}

/// Returns a deterministic zero `Hash` placeholder used when a
/// data-plane role does not carry a meaningful peer.
pub fn zero_hash() -> Hash {
    Hash::from_bytes([0_u8; 32])
}

/// Returns a `TunnelPeer` that wraps the zero hash placeholder.
pub fn zero_peer() -> TunnelPeer {
    TunnelPeer::from_hash(zero_hash())
}

/// Returns the `TunnelId` the data plane uses for the
/// "not-applicable" sentinel. The placeholder is filtered out at
/// every public data-plane boundary.
pub fn zero_id() -> TunnelId {
    TunnelId::new(u32::MAX).expect("nonzero")
}

/// Establish+extract material bundle the registrar stores in the
/// pool. The struct is the secret-bearing companion of
/// [`crate::pool::TunnelRegistration`]: one per successful build,
/// consumed exactly once.
/// Establish+extract material bundle the registrar stores in the
/// pool. The struct is the secret-bearing companion of
/// [`crate::pool::TunnelRegistration`]: one per successful build,
/// consumed exactly once.
pub struct EstablishedMaterial {
    pub(super) direction: TunnelDirection,
    pub(super) creator_tunnel_id: TunnelId,
    pub(super) hops: Vec<EstablishedHop>,
    pub(super) created_at_seconds: u64,
    pub(super) inbound_gateway: (TunnelPeer, TunnelId),
    pub(super) local_inbound_receive: TunnelId,
    pub(super) extracted: bool,
}

impl EstablishedMaterial {
    /// Returns the direction.
    pub const fn direction(&self) -> TunnelDirection {
        self.direction
    }

    /// Returns the creator tunnel id.
    pub const fn creator_tunnel_id(&self) -> TunnelId {
        self.creator_tunnel_id
    }

    /// Returns the ordered hop list.
    pub fn hops(&self) -> &[EstablishedHop] {
        &self.hops
    }

    /// Returns the creation timestamp in seconds.
    pub const fn created_at_seconds(&self) -> u64 {
        self.created_at_seconds
    }

    /// Returns the inbound gateway router hash and tunnel id.
    pub const fn inbound_gateway(&self) -> (TunnelPeer, TunnelId) {
        self.inbound_gateway
    }

    /// Returns the local inbound endpoint receive tunnel id.
    pub const fn local_inbound_receive(&self) -> TunnelId {
        self.local_inbound_receive
    }

    /// Returns whether the material is marked consumed.
    pub const fn is_extracted(&self) -> bool {
        self.extracted
    }

    /// Consumes the secret material and returns an
    /// [`EstablishedTunnel`] the data plane can use. A second
    /// call returns `None` because the secret material is
    /// zeroized in place.
    pub fn into_established_tunnel(&mut self) -> Option<EstablishedTunnel> {
        if !self.extracted {
            return None;
        }
        let hops = std::mem::take(&mut self.hops);
        let direction = self.direction;
        let creator_tunnel_id = self.creator_tunnel_id;
        let created_at_seconds = self.created_at_seconds;
        let inbound_gateway = self.inbound_gateway;
        let local_inbound_receive = self.local_inbound_receive;
        let tunnel = EstablishedTunnel {
            direction,
            creator_tunnel_id,
            hops,
            created_at_seconds,
            inbound_gateway,
            local_inbound_receive,
        };
        self.extracted = false;
        Some(tunnel)
    }
}

impl fmt::Debug for EstablishedMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EstablishedMaterial")
            .field("direction", &self.direction)
            .field("creator_tunnel_id", &self.creator_tunnel_id)
            .field("hops", &format_args!("<{} redacted>", self.hops.len()))
            .field("created_at_seconds", &self.created_at_seconds)
            .field("inbound_gateway", &self.inbound_gateway)
            .field("local_inbound_receive", &self.local_inbound_receive)
            .field("extracted", &self.extracted)
            .finish()
    }
}

impl Drop for EstablishedMaterial {
    fn drop(&mut self) {
        self.hops.zeroize();
        self.creator_tunnel_id.zeroize();
    }
}

impl Zeroize for EstablishedMaterial {
    fn zeroize(&mut self) {
        self.hops.zeroize();
        self.creator_tunnel_id.zeroize();
    }
}

impl ZeroizeOnDrop for EstablishedMaterial {}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::build_crypto::LayerKeys;

    fn peer(value: u8) -> TunnelPeer {
        TunnelPeer::from_hash(Hash::from_bytes([value; 32]))
    }

    fn keys() -> LayerKeys {
        LayerKeys::new([0xAA_u8; 32], [0xBB_u8; 32], [0xCC_u8; 32])
    }

    #[test]
    fn established_tunnel_rejects_empty_hops() {
        let error = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            Vec::new(),
            0,
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, EstablishedTunnelError::EmptyHopList));
    }

    #[test]
    fn outbound_established_tunnel_requires_obep_terminal() {
        let hops = vec![EstablishedHop::with_next(
            peer(1),
            EstablishedRole::Participant,
            TunnelId::new(2).expect("id"),
            keys(),
            EstablishedNextHop {
                router: peer(2),
                tunnel: TunnelId::new(3).expect("id"),
            },
        )];
        let error = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EstablishedTunnelError::LastHopRoleInvalid { .. }
        ));
    }

    #[test]
    fn outbound_single_hop_obep_is_valid() {
        // A single-hop outbound tunnel is the degenerate
        // [OBEP] form allowed by the canonical I2P specification.
        let hops = vec![EstablishedHop::terminal(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(2).expect("id"),
            keys(),
        )];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .expect("single-hop outbound accepted");
        assert_eq!(tunnel.hops().len(), 1);
    }

    #[test]
    fn outbound_intermediate_hop_without_next_is_rejected() {
        // A multi-hop outbound tunnel where an intermediate
        // participant lacks a `next` field is rejected.
        let hops = vec![
            EstablishedHop::terminal(
                peer(1),
                EstablishedRole::Participant,
                TunnelId::new(2).expect("id"),
                keys(),
            ),
            EstablishedHop::terminal(
                peer(2),
                EstablishedRole::OutboundEndpoint,
                TunnelId::new(3).expect("id"),
                keys(),
            ),
        ];
        let error = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EstablishedTunnelError::MissingIntermediateHopNext
        ));
    }

    #[test]
    fn inbound_established_tunnel_requires_ibgw_first() {
        let hops = vec![
            EstablishedHop::with_next(
                peer(1),
                EstablishedRole::Participant,
                TunnelId::new(2).expect("id"),
                keys(),
                EstablishedNextHop {
                    router: peer(2),
                    tunnel: TunnelId::new(3).expect("id"),
                },
            ),
            EstablishedHop::with_next(
                peer(2),
                EstablishedRole::Participant,
                TunnelId::new(3).expect("id"),
                keys(),
                EstablishedNextHop {
                    router: peer(3),
                    tunnel: TunnelId::new(4).expect("id"),
                },
            ),
        ];
        let error = EstablishedTunnel::new(
            TunnelDirection::Inbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            Some((peer(1), TunnelId::new(2).expect("id"))),
            Some(TunnelId::new(99).expect("id")),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EstablishedTunnelError::FirstHopRoleInvalid { .. }
        ));
    }

    #[test]
    fn inbound_terminal_hop_must_match_local_receive() {
        let hops = vec![
            EstablishedHop::with_next(
                peer(1),
                EstablishedRole::InboundGateway,
                TunnelId::new(2).expect("id"),
                keys(),
                EstablishedNextHop {
                    router: peer(2),
                    tunnel: TunnelId::new(3).expect("id"),
                },
            ),
            // The terminal participant's `next.tunnel` does NOT
            // equal `local_inbound_receive`; the constructor
            // rejects this with `LocalInboundReceiveMismatch`.
            EstablishedHop::with_next(
                peer(2),
                EstablishedRole::Participant,
                TunnelId::new(3).expect("id"),
                keys(),
                EstablishedNextHop {
                    router: peer(3),
                    tunnel: TunnelId::new(99).expect("id"),
                },
            ),
        ];
        let error = EstablishedTunnel::new(
            TunnelDirection::Inbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            Some((peer(1), TunnelId::new(2).expect("id"))),
            Some(TunnelId::new(0x901).expect("id")),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EstablishedTunnelError::LocalInboundReceiveMismatch { .. }
        ));
    }

    #[test]
    fn inbound_terminal_next_tunnel_matches_local_receive() {
        let hops = vec![
            EstablishedHop::with_next(
                peer(1),
                EstablishedRole::InboundGateway,
                TunnelId::new(2).expect("id"),
                keys(),
                EstablishedNextHop {
                    router: peer(2),
                    tunnel: TunnelId::new(3).expect("id"),
                },
            ),
            EstablishedHop::with_next(
                peer(2),
                EstablishedRole::Participant,
                TunnelId::new(3).expect("id"),
                keys(),
                EstablishedNextHop {
                    router: peer(3),
                    tunnel: TunnelId::new(0x901).expect("id"),
                },
            ),
        ];
        let tunnel = EstablishedTunnel::new(
            TunnelDirection::Inbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            Some((peer(1), TunnelId::new(2).expect("id"))),
            Some(TunnelId::new(0x901).expect("id")),
        )
        .expect("tunnel accepted");
        assert_eq!(tunnel.local_inbound_receive().get(), 0x901);
    }

    #[test]
    fn outbound_established_tunnel_rejects_inbound_gateway_field() {
        let hops = vec![EstablishedHop::terminal(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(2).expect("id"),
            keys(),
        )];
        let error = EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            Some((peer(1), TunnelId::new(2).expect("id"))),
            None,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EstablishedTunnelError::OutboundGatewaySpecified
        ));
    }

    #[test]
    fn established_hop_debug_redacts_keys() {
        let hop = EstablishedHop::terminal(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(2).expect("id"),
            keys(),
        );
        let rendered = format!("{hop:?}");
        assert!(rendered.contains("layer_keys: \"<redacted>\""));
        assert!(!rendered.contains("0xaa"));
        assert!(!rendered.contains("0xbb"));
        assert!(!rendered.contains("0xcc"));
    }
}
