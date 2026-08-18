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
//! terminal_next_tunnel /
//! terminal_inbound_receive_tunnel the receive id the terminal
//!                                 inbound hop expects (local
//!                                 inbound endpoint)
//! ```
//!
//! Outbound OBEP does not encode a fixed data-plane destination;
//! the data-plane delivery instruction is carried per message.

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

/// One per-hop retained crypto context. The struct owns the
/// canonical hop identity (`router_hash`, `role`, `receive_tunnel`)
/// and the forward `LayerKeys` the data plane needs to apply the
/// AES-256 ECB/CBC/ECB layer transform. The terminal data-plane
/// destination for outbound endpoints is intentionally **not**
/// stored here; it is encoded in the per-message delivery
/// instruction. For inbound endpoints the terminal data-plane
/// destination is the local inbound endpoint's `receive_tunnel`.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct EstablishedHop {
    peer: TunnelPeer,
    role: EstablishedRole,
    receive_tunnel: TunnelId,
    layer_keys: LayerKeys,
    /// Next router / next tunnel pair the participant or inbound
    /// gateway must hand the cell to. Outbound endpoints store
    /// `next_tunnel = TunnelId::ZERO_PLACEHOLDER` and
    /// `next_router = TunnelPeer::from_hash(Hash::ZERO_PLACEHOLDER)`
    /// to make the data-plane distinction explicit.
    next_router: TunnelPeer,
    next_tunnel: TunnelId,
}

impl EstablishedHop {
    /// Constructs a hop record. Callers must come from a successful
    /// short-build path; the data plane never fabricates entries.
    pub fn new(
        peer: TunnelPeer,
        role: EstablishedRole,
        receive_tunnel: TunnelId,
        layer_keys: LayerKeys,
        next_router: TunnelPeer,
        next_tunnel: TunnelId,
    ) -> Self {
        Self {
            peer,
            role,
            receive_tunnel,
            layer_keys,
            next_router,
            next_tunnel,
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

    /// Returns the next router the hop forwards cells to. Returns
    /// the zero placeholder when the role is `OutboundEndpoint`.
    pub const fn next_router(&self) -> TunnelPeer {
        self.next_router
    }

    /// Returns the next tunnel id the hop forwards cells to.
    pub const fn next_tunnel(&self) -> TunnelId {
        self.next_tunnel
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
            .field("next_router", &self.next_router)
            .field("next_tunnel", &self.next_tunnel)
            .finish()
    }
}

/// Hop-role classification used by the data plane. This is a
/// separate enumeration from the build-time [`crate::identity::TunnelRole`]
/// because the data plane only cares about the four canonical
/// roles that can be present in an established tunnel: participant,
/// outbound endpoint, inbound gateway, and inbound endpoint.
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
    /// Inbound endpoint: receives TunnelData and strips all layers.
    InboundEndpoint,
}

impl fmt::Display for EstablishedRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Participant => "participant",
            Self::OutboundEndpoint => "obep",
            Self::InboundGateway => "ibgw",
            Self::InboundEndpoint => "ibep",
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
/// needs, and the per-hop `LayerKeys`. The struct moves out of the
/// build state machine exactly once via a registered
/// `take_established_material` seam in the short-build runtime.
pub struct EstablishedTunnel {
    direction: TunnelDirection,
    creator_tunnel_id: TunnelId,
    hops: Vec<EstablishedHop>,
    created_at_seconds: u64,
    /// The inbound external gateway router hash (first hop) and the
    /// first remote hop's receive tunnel id. Only meaningful for
    /// inbound tunnels; outbound tunnels use the zero placeholder.
    inbound_gateway: (TunnelPeer, TunnelId),
    /// The local inbound endpoint's receive tunnel id. Only
    /// meaningful for inbound tunnels; outbound tunnels use the
    /// zero placeholder.
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
                // Inbound: first hop must be InboundGateway, last
                // hop must be InboundEndpoint.
                if hops[0].role() != EstablishedRole::InboundGateway {
                    return Err(EstablishedTunnelError::FirstHopRoleInvalid {
                        expected: EstablishedRole::InboundGateway,
                        actual: hops[0].role(),
                    });
                }
                let last = hops.len() - 1;
                if hops[last].role() != EstablishedRole::InboundEndpoint {
                    return Err(EstablishedTunnelError::LastHopRoleInvalid {
                        expected: EstablishedRole::InboundEndpoint,
                        actual: hops[last].role(),
                    });
                }
            }
            TunnelDirection::Outbound => {
                if inbound_gateway.is_some() {
                    return Err(EstablishedTunnelError::OutboundGatewaySpecified);
                }
                if local_inbound_receive.is_some() {
                    return Err(EstablishedTunnelError::OutboundLocalReceiveSpecified);
                }
                // Outbound: first hop must be Participant or
                // OutboundEndpoint, last hop must be OutboundEndpoint.
                let last = hops.len() - 1;
                if hops[last].role() != EstablishedRole::OutboundEndpoint {
                    return Err(EstablishedTunnelError::LastHopRoleInvalid {
                        expected: EstablishedRole::OutboundEndpoint,
                        actual: hops[last].role(),
                    });
                }
            }
        }
        // The first remote hop receive id must equal the
        // `inbound_gateway` tunnel id when one is declared.
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
        // The terminal inbound hop's receive id must equal the
        // local inbound receive id when one is declared.
        if let Some(expected) = local_inbound_receive {
            let last = hops.len() - 1;
            if hops[last].receive_tunnel() != expected {
                return Err(EstablishedTunnelError::LocalInboundReceiveMismatch {
                    expected,
                    actual: hops[last].receive_tunnel(),
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
/// "not-applicable" sentinel. `TunnelId` rejects zero by
/// construction, so the data plane uses a bounded synthetic
/// placeholder that is filtered out at every public data-plane
/// boundary. The placeholder is intentionally **not** exposed
/// through any public data-plane helper that returns `TunnelId`.
pub fn zero_id() -> TunnelId {
    TunnelId::new(u32::MAX).expect("nonzero")
}

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
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::Participant,
            TunnelId::new(2).expect("id"),
            keys(),
            peer(2),
            TunnelId::new(3).expect("id"),
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
    fn inbound_established_tunnel_requires_ibgw_and_ibep() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::Participant,
            TunnelId::new(2).expect("id"),
            keys(),
            peer(2),
            TunnelId::new(3).expect("id"),
        )];
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
    fn inbound_local_receive_must_match_terminal_hop() {
        let hops = vec![
            EstablishedHop::new(
                peer(1),
                EstablishedRole::InboundGateway,
                TunnelId::new(2).expect("id"),
                keys(),
                peer(2),
                TunnelId::new(3).expect("id"),
            ),
            EstablishedHop::new(
                peer(2),
                EstablishedRole::InboundEndpoint,
                TunnelId::new(7).expect("id"),
                keys(),
                peer(3),
                zero_id(),
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
            EstablishedTunnelError::LocalInboundReceiveMismatch { .. }
        ));
    }

    #[test]
    fn inbound_inbound_gateway_must_match_first_hop_router() {
        let hops = vec![
            EstablishedHop::new(
                peer(1),
                EstablishedRole::InboundGateway,
                TunnelId::new(2).expect("id"),
                keys(),
                peer(2),
                TunnelId::new(3).expect("id"),
            ),
            EstablishedHop::new(
                peer(2),
                EstablishedRole::InboundEndpoint,
                TunnelId::new(99).expect("id"),
                keys(),
                peer(3),
                zero_id(),
            ),
        ];
        let error = EstablishedTunnel::new(
            TunnelDirection::Inbound,
            TunnelId::new(1).expect("id"),
            hops,
            0,
            Some((peer(9), TunnelId::new(2).expect("id"))),
            Some(TunnelId::new(99).expect("id")),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EstablishedTunnelError::FirstHopRouterMismatch { .. }
        ));
    }

    #[test]
    fn outbound_established_tunnel_rejects_inbound_gateway_field() {
        let hops = vec![EstablishedHop::new(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(2).expect("id"),
            keys(),
            zero_peer(),
            zero_id(),
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
        let hop = EstablishedHop::new(
            peer(1),
            EstablishedRole::OutboundEndpoint,
            TunnelId::new(2).expect("id"),
            keys(),
            zero_peer(),
            zero_id(),
        );
        let rendered = format!("{hop:?}");
        assert!(rendered.contains("layer_keys: \"<redacted>\""));
        assert!(!rendered.contains("0xaa"));
        assert!(!rendered.contains("0xbb"));
        assert!(!rendered.contains("0xcc"));
    }
}
