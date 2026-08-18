//! Tunnel identity types.
//!
//! Plan 107 §3.1 owns the bounded typed values that identify a tunnel
//! slot, declare its direction, its participant role, and its
//! remaining lifetime. These types deliberately carry no networking,
//! runtime, or filesystem state — they are pure values that the
//! exploratory pool, the build state machine, and the reply-path
//! provider all share.

#![forbid(unsafe_code)]

use std::fmt;

use i2pr_proto::Hash;
use zeroize::Zeroize;

/// The hard ceiling on the per-tunnel identifier accepted by this
/// crate. I2P reserves the value zero to mean "no tunnel"; the
/// codec and the pool both reject it.
pub const MAX_TUNNEL_ID: u32 = u32::MAX;

/// A non-zero tunnel identifier.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TunnelId(u32);

impl Zeroize for TunnelId {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl TunnelId {
    /// Constructs a tunnel identifier, rejecting zero.
    pub const fn new(value: u32) -> Result<Self, TunnelIdError> {
        if value == 0 {
            return Err(TunnelIdError::Zero);
        }
        Ok(Self(value))
    }

    /// Returns the inner numeric value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Debug for TunnelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("TunnelId")
            .field(&format_args!("{:08x}", self.0))
            .finish()
    }
}

impl fmt::Display for TunnelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:08x}", self.0)
    }
}

/// Validation failures for [`TunnelId`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelIdError {
    /// The supplied value was zero.
    Zero,
}

impl fmt::Display for TunnelIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("tunnel id must be nonzero"),
        }
    }
}

impl std::error::Error for TunnelIdError {}

/// Direction of a tunnel relative to the local router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelDirection {
    /// Traffic enters the local router through this tunnel.
    Inbound,
    /// Traffic leaves the local router through this tunnel.
    Outbound,
}

impl fmt::Display for TunnelDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        };
        formatter.write_str(label)
    }
}

/// Role the local router plays in this tunnel.
///
/// Plan 107 recognises only the four roles the I2P tunnel
/// specification defines for explorer/participant endpoints:
/// inbound gateway, participant, outbound endpoint, and the
/// local-side inbound endpoint / outbound gateway. A creator
/// (build originator) role is recorded separately on the build
/// state machine and is not a participant role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelRole {
    /// Local router is the gateway that admits messages into the
    /// tunnel (inbound direction from the local router's point of
    /// view).
    InboundGateway,
    /// Local router is one of the intermediate hops that decrypts
    /// one layer and forwards to the next peer.
    Participant,
    /// Local router is the terminal hop that strips the final
    /// tunnel layer and delivers the inner message.
    OutboundEndpoint,
    /// Local router is the endpoint that receives tunnel messages
    /// from the inbound side (inbound direction).
    InboundEndpoint,
}

impl fmt::Display for TunnelRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::InboundGateway => "inbound-gateway",
            Self::Participant => "participant",
            Self::OutboundEndpoint => "outbound-endpoint",
            Self::InboundEndpoint => "inbound-endpoint",
        };
        formatter.write_str(label)
    }
}

/// Bounded lifetime expressed in seconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TunnelLifetime {
    /// Lifetime in seconds; ceiling is [`MAX_LIFETIME_SECONDS`].
    seconds: u32,
}

impl TunnelLifetime {
    /// Hard ceiling on lifetime; mirrors the I2P default maximum.
    pub const MAX_LIFETIME_SECONDS: u32 = 30 * 60;
    /// Default exploratory tunnel lifetime.
    pub const DEFAULT_EXPLORATORY_SECONDS: u32 = 10 * 60;

    /// Constructs a lifetime value with validation.
    pub const fn from_seconds(seconds: u32) -> Result<Self, TunnelLifetimeError> {
        if seconds == 0 {
            return Err(TunnelLifetimeError::Zero);
        }
        if seconds > Self::MAX_LIFETIME_SECONDS {
            return Err(TunnelLifetimeError::ExceedsMaximum {
                actual: seconds,
                maximum: Self::MAX_LIFETIME_SECONDS,
            });
        }
        Ok(Self { seconds })
    }

    /// Returns the lifetime in seconds.
    pub const fn seconds(self) -> u32 {
        self.seconds
    }
}

/// Validation failures for [`TunnelLifetime`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelLifetimeError {
    /// Lifetime was zero seconds.
    Zero,
    /// Lifetime exceeded the documented ceiling.
    ExceedsMaximum {
        /// Actual supplied lifetime.
        actual: u32,
        /// Maximum permitted lifetime.
        maximum: u32,
    },
}

impl fmt::Display for TunnelLifetimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("tunnel lifetime must be nonzero"),
            Self::ExceedsMaximum { actual, maximum } => write!(
                formatter,
                "tunnel lifetime {actual} exceeds maximum {maximum}"
            ),
        }
    }
}

impl std::error::Error for TunnelLifetimeError {}

/// The status of a tunnel slot in the exploratory pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelState {
    /// Build has been requested and is in flight.
    Building,
    /// Tunnel is established and accepting traffic.
    Established,
    /// Tunnel has expired but is still registered for cleanup.
    Expired,
    /// Tunnel build or operation failed.
    Failed,
}

impl fmt::Display for TunnelState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Building => "building",
            Self::Established => "established",
            Self::Expired => "expired",
            Self::Failed => "failed",
        };
        formatter.write_str(label)
    }
}

/// Router hash identifying a tunnel's gateway or endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TunnelPeer(Hash);

impl Zeroize for TunnelPeer {
    fn zeroize(&mut self) {
        *self = TunnelPeer(Hash::from_bytes([0_u8; 32]));
    }
}

impl TunnelPeer {
    /// Constructs a tunnel peer from a router hash.
    pub const fn from_hash(hash: Hash) -> Self {
        Self(hash)
    }

    /// Returns the wrapped router hash.
    pub const fn hash(&self) -> Hash {
        self.0
    }
}

impl fmt::Display for TunnelPeer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("TunnelPeer").field(&self.0).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_id_rejects_zero() {
        assert_eq!(TunnelId::new(0), Err(TunnelIdError::Zero));
        let id = TunnelId::new(7).expect("nonzero");
        assert_eq!(id.get(), 7);
    }

    #[test]
    fn tunnel_lifetime_rejects_zero_and_over_max() {
        assert_eq!(
            TunnelLifetime::from_seconds(0),
            Err(TunnelLifetimeError::Zero)
        );
        let over = TunnelLifetime::MAX_LIFETIME_SECONDS + 1;
        assert_eq!(
            TunnelLifetime::from_seconds(over),
            Err(TunnelLifetimeError::ExceedsMaximum {
                actual: over,
                maximum: TunnelLifetime::MAX_LIFETIME_SECONDS
            })
        );
        let ok = TunnelLifetime::from_seconds(600).expect("ok");
        assert_eq!(ok.seconds(), 600);
    }

    #[test]
    fn tunnel_state_display_is_stable() {
        assert_eq!(TunnelState::Building.to_string(), "building");
        assert_eq!(TunnelState::Established.to_string(), "established");
        assert_eq!(TunnelState::Expired.to_string(), "expired");
        assert_eq!(TunnelState::Failed.to_string(), "failed");
    }

    #[test]
    fn tunnel_role_display_is_stable() {
        assert_eq!(TunnelRole::InboundGateway.to_string(), "inbound-gateway");
        assert_eq!(TunnelRole::Participant.to_string(), "participant");
        assert_eq!(
            TunnelRole::OutboundEndpoint.to_string(),
            "outbound-endpoint"
        );
        assert_eq!(TunnelRole::InboundEndpoint.to_string(), "inbound-endpoint");
    }
}
