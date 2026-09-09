//! Explicit local zero-hop tunnel representation (Plan 172 §5).
//!
//! A zero-hop tunnel is a normal I2P tunnel form where gateway and
//! endpoint are the same router. It carries no remote peer vector
//! and no remote hop [`crate::build_crypto::LayerKeys`]. The type
//! below is deliberately distinct from [`crate::established::EstablishedMaterial`]
//! so an empty remote hop list can never be mistaken for a usable
//! local path.
//!
//! Invariants (all enforced at construction):
//!
//! - gateway is the actual local router hash (never all-zero);
//! - tunnel id is fresh, non-zero, and never the `u32::MAX` sentinel;
//! - no fabricated layer keys are stored;
//! - entries are never offered to the remote participant/IBGW/OBEP
//!   crypto data plane;
//! - remote [`crate::established::EstablishedTunnel::new`] still
//!   rejects an empty hop list and [`crate::config::MIN_HOPS`]
//!   remains 1.
//!
//! The crate stays runtime-neutral: no sockets, no Tokio, no timers.

#![forbid(unsafe_code)]

use core::fmt;

use i2pr_proto::Hash;

use crate::identity::{TunnelDirection, TunnelId};

/// Maximum lifetime for a local zero-hop entry in seconds (10 minutes,
/// matching the exploratory default window).
pub const MAX_ZERO_HOP_LIFETIME_SECONDS: u32 = 600;

/// Local zero-hop inbound tunnel: gateway == endpoint == this router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalZeroHopInbound {
    gateway: Hash,
    receive_tunnel: TunnelId,
    created_at_seconds: u64,
    expires_seconds: u64,
}

impl LocalZeroHopInbound {
    /// Constructs a local zero-hop inbound entry.
    pub fn new(
        gateway: Hash,
        receive_tunnel: TunnelId,
        created_at_seconds: u64,
        lifetime_seconds: u32,
    ) -> Result<Self, ZeroHopError> {
        if gateway == Hash::from_bytes([0; 32]) {
            return Err(ZeroHopError::ZeroGateway);
        }
        if receive_tunnel.get() == 0 {
            return Err(ZeroHopError::ZeroTunnelId);
        }
        if receive_tunnel.get() == u32::MAX {
            return Err(ZeroHopError::SentinelTunnelId);
        }
        if lifetime_seconds == 0 || lifetime_seconds > MAX_ZERO_HOP_LIFETIME_SECONDS {
            return Err(ZeroHopError::LifetimeOutOfRange {
                actual: lifetime_seconds,
                maximum: MAX_ZERO_HOP_LIFETIME_SECONDS,
            });
        }
        let expires_seconds = created_at_seconds.saturating_add(u64::from(lifetime_seconds));
        Ok(Self {
            gateway,
            receive_tunnel,
            created_at_seconds,
            expires_seconds,
        })
    }

    /// Gateway router hash (== local router).
    pub const fn gateway(&self) -> Hash {
        self.gateway
    }

    /// Local receive tunnel id advertised in the Lease.
    pub const fn receive_tunnel(&self) -> TunnelId {
        self.receive_tunnel
    }

    /// Creation time in seconds.
    pub const fn created_at_seconds(&self) -> u64 {
        self.created_at_seconds
    }

    /// Expiry time in seconds.
    pub const fn expires_seconds(&self) -> u64 {
        self.expires_seconds
    }

    /// Direction is always inbound for this type.
    pub const fn direction(&self) -> TunnelDirection {
        TunnelDirection::Inbound
    }

    /// Whether the entry is usable at `now_seconds`.
    pub const fn is_usable(&self, now_seconds: u64) -> bool {
        now_seconds < self.expires_seconds
    }
}

/// Local zero-hop outbound path: an honest local route, not a
/// fabricated remote peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalZeroHopOutbound {
    local_route: TunnelId,
    created_at_seconds: u64,
    expires_seconds: u64,
}

impl LocalZeroHopOutbound {
    /// Constructs a local zero-hop outbound entry.
    pub fn new(
        local_route: TunnelId,
        created_at_seconds: u64,
        lifetime_seconds: u32,
    ) -> Result<Self, ZeroHopError> {
        if local_route.get() == 0 {
            return Err(ZeroHopError::ZeroTunnelId);
        }
        if local_route.get() == u32::MAX {
            return Err(ZeroHopError::SentinelTunnelId);
        }
        if lifetime_seconds == 0 || lifetime_seconds > MAX_ZERO_HOP_LIFETIME_SECONDS {
            return Err(ZeroHopError::LifetimeOutOfRange {
                actual: lifetime_seconds,
                maximum: MAX_ZERO_HOP_LIFETIME_SECONDS,
            });
        }
        let expires_seconds = created_at_seconds.saturating_add(u64::from(lifetime_seconds));
        Ok(Self {
            local_route,
            created_at_seconds,
            expires_seconds,
        })
    }

    /// Local route id.
    pub const fn local_route(&self) -> TunnelId {
        self.local_route
    }

    /// Creation time in seconds.
    pub const fn created_at_seconds(&self) -> u64 {
        self.created_at_seconds
    }

    /// Expiry time in seconds.
    pub const fn expires_seconds(&self) -> u64 {
        self.expires_seconds
    }

    /// Direction is always outbound for this type.
    pub const fn direction(&self) -> TunnelDirection {
        TunnelDirection::Outbound
    }

    /// Whether the entry is usable at `now_seconds`.
    pub const fn is_usable(&self, now_seconds: u64) -> bool {
        now_seconds < self.expires_seconds
    }
}

/// Typed construction failures for zero-hop entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ZeroHopError {
    /// Gateway hash was all-zero.
    ZeroGateway,
    /// Tunnel id was zero.
    ZeroTunnelId,
    /// Tunnel id was the `u32::MAX` sentinel.
    SentinelTunnelId,
    /// Lifetime was zero or exceeded the ceiling.
    LifetimeOutOfRange {
        /// Supplied lifetime.
        actual: u32,
        /// Accepted ceiling.
        maximum: u32,
    },
}

impl fmt::Display for ZeroHopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroGateway => formatter.write_str("zero-hop gateway must not be all-zero"),
            Self::ZeroTunnelId => formatter.write_str("zero-hop tunnel id must be non-zero"),
            Self::SentinelTunnelId => {
                formatter.write_str("zero-hop tunnel id must not be the u32::MAX sentinel")
            }
            Self::LifetimeOutOfRange { actual, maximum } => write!(
                formatter,
                "zero-hop lifetime {actual} out of range (maximum {maximum})"
            ),
        }
    }
}

impl std::error::Error for ZeroHopError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway(value: u8) -> Hash {
        Hash::from_bytes([value; 32])
    }

    #[test]
    fn inbound_rejects_zero_gateway() {
        let id = TunnelId::new(0x1000).expect("id");
        let err = LocalZeroHopInbound::new(Hash::from_bytes([0; 32]), id, 0, 60).unwrap_err();
        assert_eq!(err, ZeroHopError::ZeroGateway);
    }

    #[test]
    fn inbound_rejects_sentinel_tunnel_id() {
        let sentinel = TunnelId::new(u32::MAX).expect("nonzero");
        let err = LocalZeroHopInbound::new(gateway(1), sentinel, 0, 60).unwrap_err();
        assert_eq!(err, ZeroHopError::SentinelTunnelId);
    }

    #[test]
    fn outbound_rejects_zero_and_sentinel() {
        let zero = TunnelId::new(1).expect("id");
        let _ = zero;
        // TunnelId::new(0) is None so zero cannot be constructed; sentinel is rejected.
        let sentinel = TunnelId::new(u32::MAX).expect("nonzero");
        let err = LocalZeroHopOutbound::new(sentinel, 0, 60).unwrap_err();
        assert_eq!(err, ZeroHopError::SentinelTunnelId);
    }

    #[test]
    fn usable_tracks_expiry() {
        let id = TunnelId::new(0x1000).expect("id");
        let entry = LocalZeroHopInbound::new(gateway(2), id, 0, 60).expect("entry");
        assert!(entry.is_usable(0));
        assert!(entry.is_usable(59));
        assert!(!entry.is_usable(60));
    }

    #[test]
    fn remote_empty_hop_list_still_rejected() {
        // Plan 172 §5 invariant: the remote constructor is untouched.
        let err = crate::established::EstablishedTunnel::new(
            TunnelDirection::Outbound,
            TunnelId::new(1).expect("id"),
            Vec::new(),
            0,
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(
            err,
            crate::established::EstablishedTunnelError::EmptyHopList
        );
    }
}
