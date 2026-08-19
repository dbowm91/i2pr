//! Bounded destination configuration and centralized resource defaults.
//!
//! Plan 120 §13 requires every destination resource default to live in one
//! place and be test-overridable. Every ceiling below is a documented hard
//! bound; a configuration that exceeds any ceiling is rejected with a typed
//! [`DestinationConfigError`].

use core::fmt;

use i2pr_tunnel::{BoundedTunnelPoolConfig, ExploratoryConfigError, TunnelLifetime};

/// Maximum number of local destinations a single router may own.
pub const MAX_LOCAL_DESTINATIONS: u16 = 16;
/// Maximum number of inbound tunnels a single destination may own.
pub const MAX_DESTINATION_INBOUND: u16 = 8;
/// Maximum number of outbound tunnels a single destination may own.
pub const MAX_DESTINATION_OUTBOUND: u16 = 8;
/// Maximum number of simultaneous builds/replacements per destination.
pub const MAX_DESTINATION_BUILD_CONCURRENCY: u16 = 4;
/// Maximum number of consecutive failed builds tolerated per destination.
pub const MAX_DESTINATION_FAILURE_THRESHOLD: u16 = 16;
/// Maximum number of pending application payloads retained per destination.
pub const MAX_PENDING_DESTINATION_MESSAGES: u16 = 256;
/// Maximum aggregate pending application payload bytes per destination.
pub const MAX_PENDING_DESTINATION_BYTES: usize = 512 * 1024;
/// Maximum aggregate registry command-queue depth across all destinations.
pub const MAX_AGGREGATE_COMMAND_QUEUE_DEPTH: u32 = 4096;
/// Hard ceiling on the publication safety margin subtracted from a tunnel's
/// real expiry before it is advertised in a `Lease2`.
pub const MAX_LEASE_PUBLICATION_MARGIN_SECONDS: u32 = 600;
/// Hard ceiling on the rotation margin that triggers LeaseSet2 replacement
/// before the advertised leases expire.
pub const MAX_LEASE_ROTATION_MARGIN_SECONDS: u32 = 600;

/// Default publication safety margin. An advertised lease always ends this
/// many seconds before the underlying tunnel actually expires so remote
/// routers never route to a dead gateway.
pub const DEFAULT_LEASE_PUBLICATION_MARGIN_SECONDS: u32 = 60;
/// Default rotation margin. The local LeaseSet2 is regenerated once the
/// earliest advertised lease is within this window of expiry.
pub const DEFAULT_LEASE_ROTATION_MARGIN_SECONDS: u32 = 120;

/// Bounded per-destination configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestinationConfig {
    inbound_target: u16,
    outbound_target: u16,
    minimum_usable_inbound: u16,
    length_hops: u8,
    tunnel_lifetime_seconds: u32,
    build_concurrency: u16,
    failure_threshold: u16,
    max_pending_messages: u16,
    max_pending_bytes: usize,
    lease_publication_margin_seconds: u32,
    lease_rotation_margin_seconds: u32,
}

impl DestinationConfig {
    /// Conservative experimental defaults: two inbound and two outbound
    /// tunnels, one minimum usable inbound tunnel, two hops each.
    pub fn balanced() -> Self {
        Self::try_new(
            2,
            2,
            1,
            2,
            TunnelLifetime::DEFAULT_EXPLORATORY_SECONDS,
            2,
            8,
            64,
            128 * 1024,
            DEFAULT_LEASE_PUBLICATION_MARGIN_SECONDS,
            DEFAULT_LEASE_ROTATION_MARGIN_SECONDS,
        )
        .expect("balanced destination configuration is within every ceiling")
    }

    /// Builds a configuration after applying every documented ceiling.
    #[allow(clippy::too_many_arguments)]
    pub const fn try_new(
        inbound_target: u16,
        outbound_target: u16,
        minimum_usable_inbound: u16,
        length_hops: u8,
        tunnel_lifetime_seconds: u32,
        build_concurrency: u16,
        failure_threshold: u16,
        max_pending_messages: u16,
        max_pending_bytes: usize,
        lease_publication_margin_seconds: u32,
        lease_rotation_margin_seconds: u32,
    ) -> Result<Self, DestinationConfigError> {
        if inbound_target == 0 {
            return Err(DestinationConfigError::ZeroInboundTarget);
        }
        if inbound_target > MAX_DESTINATION_INBOUND {
            return Err(DestinationConfigError::InboundExceedsMaximum {
                actual: inbound_target,
                maximum: MAX_DESTINATION_INBOUND,
            });
        }
        if outbound_target == 0 {
            return Err(DestinationConfigError::ZeroOutboundTarget);
        }
        if outbound_target > MAX_DESTINATION_OUTBOUND {
            return Err(DestinationConfigError::OutboundExceedsMaximum {
                actual: outbound_target,
                maximum: MAX_DESTINATION_OUTBOUND,
            });
        }
        if minimum_usable_inbound == 0 {
            return Err(DestinationConfigError::ZeroMinimumUsableInbound);
        }
        if minimum_usable_inbound > inbound_target {
            return Err(DestinationConfigError::MinimumUsableExceedsTarget {
                minimum: minimum_usable_inbound,
                target: inbound_target,
            });
        }
        if build_concurrency == 0 {
            return Err(DestinationConfigError::ZeroBuildConcurrency);
        }
        if build_concurrency > MAX_DESTINATION_BUILD_CONCURRENCY {
            return Err(DestinationConfigError::BuildConcurrencyExceedsMaximum {
                actual: build_concurrency,
                maximum: MAX_DESTINATION_BUILD_CONCURRENCY,
            });
        }
        if failure_threshold == 0 {
            return Err(DestinationConfigError::ZeroFailureThreshold);
        }
        if failure_threshold > MAX_DESTINATION_FAILURE_THRESHOLD {
            return Err(DestinationConfigError::FailureThresholdExceedsMaximum {
                actual: failure_threshold,
                maximum: MAX_DESTINATION_FAILURE_THRESHOLD,
            });
        }
        if max_pending_messages == 0 {
            return Err(DestinationConfigError::ZeroPendingMessages);
        }
        if max_pending_messages > MAX_PENDING_DESTINATION_MESSAGES {
            return Err(DestinationConfigError::PendingMessagesExceedsMaximum {
                actual: max_pending_messages,
                maximum: MAX_PENDING_DESTINATION_MESSAGES,
            });
        }
        if max_pending_bytes == 0 {
            return Err(DestinationConfigError::ZeroPendingBytes);
        }
        if max_pending_bytes > MAX_PENDING_DESTINATION_BYTES {
            return Err(DestinationConfigError::PendingBytesExceedsMaximum {
                actual: max_pending_bytes,
                maximum: MAX_PENDING_DESTINATION_BYTES,
            });
        }
        if lease_publication_margin_seconds > MAX_LEASE_PUBLICATION_MARGIN_SECONDS {
            return Err(DestinationConfigError::PublicationMarginExceedsMaximum {
                actual: lease_publication_margin_seconds,
                maximum: MAX_LEASE_PUBLICATION_MARGIN_SECONDS,
            });
        }
        if lease_rotation_margin_seconds > MAX_LEASE_ROTATION_MARGIN_SECONDS {
            return Err(DestinationConfigError::RotationMarginExceedsMaximum {
                actual: lease_rotation_margin_seconds,
                maximum: MAX_LEASE_ROTATION_MARGIN_SECONDS,
            });
        }
        if lease_publication_margin_seconds >= tunnel_lifetime_seconds {
            return Err(DestinationConfigError::PublicationMarginExceedsLifetime {
                margin: lease_publication_margin_seconds,
                lifetime: tunnel_lifetime_seconds,
            });
        }
        Ok(Self {
            inbound_target,
            outbound_target,
            minimum_usable_inbound,
            length_hops,
            tunnel_lifetime_seconds,
            build_concurrency,
            failure_threshold,
            max_pending_messages,
            max_pending_bytes,
            lease_publication_margin_seconds,
            lease_rotation_margin_seconds,
        })
    }

    /// Target number of destination inbound tunnels.
    pub const fn inbound_target(&self) -> u16 {
        self.inbound_target
    }

    /// Target number of destination outbound tunnels.
    pub const fn outbound_target(&self) -> u16 {
        self.outbound_target
    }

    /// Minimum number of usable inbound tunnels required before the
    /// destination is considered `Usable` and publishable.
    pub const fn minimum_usable_inbound(&self) -> u16 {
        self.minimum_usable_inbound
    }

    /// Hop length used for destination tunnel builds.
    pub const fn length_hops(&self) -> u8 {
        self.length_hops
    }

    /// Destination tunnel lifetime in seconds.
    pub const fn tunnel_lifetime_seconds(&self) -> u32 {
        self.tunnel_lifetime_seconds
    }

    /// Maximum simultaneous builds/replacements.
    pub const fn build_concurrency(&self) -> u16 {
        self.build_concurrency
    }

    /// Consecutive build-failure threshold.
    pub const fn failure_threshold(&self) -> u16 {
        self.failure_threshold
    }

    /// Maximum pending application payloads.
    pub const fn max_pending_messages(&self) -> u16 {
        self.max_pending_messages
    }

    /// Maximum aggregate pending application payload bytes.
    pub const fn max_pending_bytes(&self) -> usize {
        self.max_pending_bytes
    }

    /// Publication safety margin subtracted from real tunnel expiry.
    pub const fn lease_publication_margin_seconds(&self) -> u32 {
        self.lease_publication_margin_seconds
    }

    /// Rotation margin that triggers LeaseSet2 replacement.
    pub const fn lease_rotation_margin_seconds(&self) -> u32 {
        self.lease_rotation_margin_seconds
    }

    /// Projects the destination policy onto the shared bounded tunnel-pool
    /// configuration owned by `i2pr-tunnel`.
    pub const fn pool_config(&self) -> Result<BoundedTunnelPoolConfig, ExploratoryConfigError> {
        BoundedTunnelPoolConfig::try_new(
            self.inbound_target,
            self.outbound_target,
            self.length_hops,
            self.tunnel_lifetime_seconds,
            self.build_concurrency,
            self.failure_threshold,
        )
    }
}

impl Default for DestinationConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Bounded router-local destination registry configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistryConfig {
    max_destinations: u16,
    max_aggregate_command_queue_depth: u32,
}

impl RegistryConfig {
    /// Builds a registry configuration after applying every ceiling.
    pub const fn try_new(
        max_destinations: u16,
        max_aggregate_command_queue_depth: u32,
    ) -> Result<Self, DestinationConfigError> {
        if max_destinations == 0 {
            return Err(DestinationConfigError::ZeroMaxDestinations);
        }
        if max_destinations > MAX_LOCAL_DESTINATIONS {
            return Err(DestinationConfigError::MaxDestinationsExceedsMaximum {
                actual: max_destinations,
                maximum: MAX_LOCAL_DESTINATIONS,
            });
        }
        if max_aggregate_command_queue_depth == 0 {
            return Err(DestinationConfigError::ZeroCommandQueueDepth);
        }
        if max_aggregate_command_queue_depth > MAX_AGGREGATE_COMMAND_QUEUE_DEPTH {
            return Err(DestinationConfigError::CommandQueueDepthExceedsMaximum {
                actual: max_aggregate_command_queue_depth,
                maximum: MAX_AGGREGATE_COMMAND_QUEUE_DEPTH,
            });
        }
        Ok(Self {
            max_destinations,
            max_aggregate_command_queue_depth,
        })
    }

    /// Maximum number of local destinations.
    pub const fn max_destinations(&self) -> u16 {
        self.max_destinations
    }

    /// Maximum aggregate command-queue depth across all destinations.
    pub const fn max_aggregate_command_queue_depth(&self) -> u32 {
        self.max_aggregate_command_queue_depth
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self::try_new(4, 1024).expect("default registry configuration is within every ceiling")
    }
}

/// Typed configuration validation failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DestinationConfigError {
    /// The inbound tunnel target was zero.
    ZeroInboundTarget,
    /// The inbound tunnel target exceeded the ceiling.
    InboundExceedsMaximum {
        /// Supplied target.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The outbound tunnel target was zero.
    ZeroOutboundTarget,
    /// The outbound tunnel target exceeded the ceiling.
    OutboundExceedsMaximum {
        /// Supplied target.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The minimum usable inbound count was zero.
    ZeroMinimumUsableInbound,
    /// The minimum usable inbound count exceeded the inbound target.
    MinimumUsableExceedsTarget {
        /// Supplied minimum.
        minimum: u16,
        /// Supplied target.
        target: u16,
    },
    /// The build concurrency was zero.
    ZeroBuildConcurrency,
    /// The build concurrency exceeded the ceiling.
    BuildConcurrencyExceedsMaximum {
        /// Supplied concurrency.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The failure threshold was zero.
    ZeroFailureThreshold,
    /// The failure threshold exceeded the ceiling.
    FailureThresholdExceedsMaximum {
        /// Supplied threshold.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The pending-message ceiling was zero.
    ZeroPendingMessages,
    /// The pending-message ceiling exceeded the maximum.
    PendingMessagesExceedsMaximum {
        /// Supplied ceiling.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The pending-byte ceiling was zero.
    ZeroPendingBytes,
    /// The pending-byte ceiling exceeded the maximum.
    PendingBytesExceedsMaximum {
        /// Supplied ceiling.
        actual: usize,
        /// Accepted ceiling.
        maximum: usize,
    },
    /// The publication margin exceeded the maximum.
    PublicationMarginExceedsMaximum {
        /// Supplied margin.
        actual: u32,
        /// Accepted ceiling.
        maximum: u32,
    },
    /// The rotation margin exceeded the maximum.
    RotationMarginExceedsMaximum {
        /// Supplied margin.
        actual: u32,
        /// Accepted ceiling.
        maximum: u32,
    },
    /// The publication margin consumed the whole tunnel lifetime.
    PublicationMarginExceedsLifetime {
        /// Supplied margin.
        margin: u32,
        /// Supplied lifetime.
        lifetime: u32,
    },
    /// The registry destination ceiling was zero.
    ZeroMaxDestinations,
    /// The registry destination ceiling exceeded the maximum.
    MaxDestinationsExceedsMaximum {
        /// Supplied ceiling.
        actual: u16,
        /// Accepted ceiling.
        maximum: u16,
    },
    /// The aggregate command-queue depth was zero.
    ZeroCommandQueueDepth,
    /// The aggregate command-queue depth exceeded the maximum.
    CommandQueueDepthExceedsMaximum {
        /// Supplied depth.
        actual: u32,
        /// Accepted ceiling.
        maximum: u32,
    },
}

impl fmt::Display for DestinationConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroInboundTarget => formatter.write_str("inbound target must be nonzero"),
            Self::InboundExceedsMaximum { actual, maximum } => {
                write!(
                    formatter,
                    "inbound target {actual} exceeds maximum {maximum}"
                )
            }
            Self::ZeroOutboundTarget => formatter.write_str("outbound target must be nonzero"),
            Self::OutboundExceedsMaximum { actual, maximum } => write!(
                formatter,
                "outbound target {actual} exceeds maximum {maximum}"
            ),
            Self::ZeroMinimumUsableInbound => {
                formatter.write_str("minimum usable inbound must be nonzero")
            }
            Self::MinimumUsableExceedsTarget { minimum, target } => write!(
                formatter,
                "minimum usable inbound {minimum} exceeds inbound target {target}"
            ),
            Self::ZeroBuildConcurrency => formatter.write_str("build concurrency must be nonzero"),
            Self::BuildConcurrencyExceedsMaximum { actual, maximum } => write!(
                formatter,
                "build concurrency {actual} exceeds maximum {maximum}"
            ),
            Self::ZeroFailureThreshold => formatter.write_str("failure threshold must be nonzero"),
            Self::FailureThresholdExceedsMaximum { actual, maximum } => write!(
                formatter,
                "failure threshold {actual} exceeds maximum {maximum}"
            ),
            Self::ZeroPendingMessages => {
                formatter.write_str("pending message ceiling must be nonzero")
            }
            Self::PendingMessagesExceedsMaximum { actual, maximum } => write!(
                formatter,
                "pending messages {actual} exceeds maximum {maximum}"
            ),
            Self::ZeroPendingBytes => formatter.write_str("pending byte ceiling must be nonzero"),
            Self::PendingBytesExceedsMaximum { actual, maximum } => {
                write!(
                    formatter,
                    "pending bytes {actual} exceeds maximum {maximum}"
                )
            }
            Self::PublicationMarginExceedsMaximum { actual, maximum } => write!(
                formatter,
                "publication margin {actual} exceeds maximum {maximum}"
            ),
            Self::RotationMarginExceedsMaximum { actual, maximum } => write!(
                formatter,
                "rotation margin {actual} exceeds maximum {maximum}"
            ),
            Self::PublicationMarginExceedsLifetime { margin, lifetime } => write!(
                formatter,
                "publication margin {margin} exceeds tunnel lifetime {lifetime}"
            ),
            Self::ZeroMaxDestinations => formatter.write_str("max destinations must be nonzero"),
            Self::MaxDestinationsExceedsMaximum { actual, maximum } => write!(
                formatter,
                "max destinations {actual} exceeds maximum {maximum}"
            ),
            Self::ZeroCommandQueueDepth => {
                formatter.write_str("aggregate command queue depth must be nonzero")
            }
            Self::CommandQueueDepthExceedsMaximum { actual, maximum } => write!(
                formatter,
                "aggregate command queue depth {actual} exceeds maximum {maximum}"
            ),
        }
    }
}

impl std::error::Error for DestinationConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_configuration_is_within_bounds() {
        let config = DestinationConfig::balanced();
        assert_eq!(config.inbound_target(), 2);
        assert_eq!(config.outbound_target(), 2);
        assert_eq!(config.minimum_usable_inbound(), 1);
        assert_eq!(config.length_hops(), 2);
        assert_eq!(config.build_concurrency(), 2);
        assert!(config.pool_config().is_ok());
    }

    #[test]
    fn configuration_rejects_zero_and_excess_values() {
        let base = DestinationConfig::balanced();
        assert_eq!(
            DestinationConfig::try_new(0, 2, 1, 2, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::ZeroInboundTarget)
        );
        assert_eq!(
            DestinationConfig::try_new(
                MAX_DESTINATION_INBOUND + 1,
                2,
                1,
                2,
                600,
                2,
                8,
                64,
                1024,
                60,
                120
            ),
            Err(DestinationConfigError::InboundExceedsMaximum {
                actual: MAX_DESTINATION_INBOUND + 1,
                maximum: MAX_DESTINATION_INBOUND,
            })
        );
        assert_eq!(
            DestinationConfig::try_new(2, 2, 3, 2, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::MinimumUsableExceedsTarget {
                minimum: 3,
                target: 2,
            })
        );
        assert_eq!(
            DestinationConfig::try_new(2, 2, 1, 2, 600, 2, 8, 0, 1024, 60, 120),
            Err(DestinationConfigError::ZeroPendingMessages)
        );
        assert_eq!(
            DestinationConfig::try_new(2, 2, 1, 2, 60, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::PublicationMarginExceedsLifetime {
                margin: 60,
                lifetime: 60,
            })
        );
        // The balanced configuration remains untouched by rejected attempts.
        assert_eq!(base, DestinationConfig::balanced());
    }

    #[test]
    fn registry_configuration_is_bounded() {
        assert_eq!(
            RegistryConfig::try_new(0, 16),
            Err(DestinationConfigError::ZeroMaxDestinations)
        );
        assert_eq!(
            RegistryConfig::try_new(MAX_LOCAL_DESTINATIONS + 1, 16),
            Err(DestinationConfigError::MaxDestinationsExceedsMaximum {
                actual: MAX_LOCAL_DESTINATIONS + 1,
                maximum: MAX_LOCAL_DESTINATIONS,
            })
        );
        let config = RegistryConfig::default();
        assert_eq!(config.max_destinations(), 4);
        assert_eq!(config.max_aggregate_command_queue_depth(), 1024);
    }
}
