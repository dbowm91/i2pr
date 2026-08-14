//! Bounded exploratory tunnel pool configuration and validation.
//!
//! Plan 107 §3.2 owns the typed configuration for the exploratory
//! tunnel pool. The configuration is intentionally narrow: a small
//! set of bounded counts, the per-pool lifetime, and the per-tunnel
//! hop length. Policy projection (diversity, peer selection) lives
//! outside the codec surface.

#![forbid(unsafe_code)]

use std::fmt;

use crate::identity::TunnelLifetime;

/// Maximum number of inbound exploratory tunnels accepted by the
/// pool. The I2P reference implementation tops out at a similar
/// value; we keep the ceiling tight to bound crypto and resource
/// pressure.
pub const MAX_EXPLORATORY_INBOUND: u16 = 8;
/// Maximum number of outbound exploratory tunnels accepted by the
/// pool.
pub const MAX_EXPLORATORY_OUTBOUND: u16 = 8;
/// Maximum number of concurrent in-flight builds.
pub const MAX_BUILD_CONCURRENCY: u16 = 4;
/// Maximum number of consecutive failures tolerated before the pool
/// stops attempting new builds.
pub const MAX_FAILURE_THRESHOLD: u16 = 16;
/// Maximum number of hops per exploratory tunnel. The I2P default is
/// two hops; the upper bound reflects transit-tunnel limits in the
/// reference router.
pub const MAX_HOPS: u8 = 8;
/// Minimum number of hops per exploratory tunnel.
pub const MIN_HOPS: u8 = 1;

/// Configuration for an exploratory tunnel pool.
///
/// The configuration is bounded: every count and length is validated
/// against a documented ceiling at construction. A configuration that
/// exceeds any ceiling is rejected with a typed
/// [`ExploratoryConfigError`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExploratoryPoolConfig {
    max_inbound: u16,
    max_outbound: u16,
    length_hops: u8,
    lifetime: TunnelLifetime,
    build_concurrency: u16,
    failure_threshold: u16,
}

impl ExploratoryPoolConfig {
    /// Construct a balanced exploratory pool configuration that matches
    /// the I2P defaults the reference router ships with.
    pub fn balanced() -> Self {
        Self {
            max_inbound: 4,
            max_outbound: 4,
            length_hops: 2,
            lifetime: TunnelLifetime::from_seconds(TunnelLifetime::DEFAULT_EXPLORATORY_SECONDS)
                .expect("default lifetime is valid"),
            build_concurrency: 2,
            failure_threshold: 8,
        }
    }

    /// Builds a configuration with the supplied counts after applying
    /// every documented ceiling.
    pub const fn try_new(
        max_inbound: u16,
        max_outbound: u16,
        length_hops: u8,
        lifetime_seconds: u32,
        build_concurrency: u16,
        failure_threshold: u16,
    ) -> Result<Self, ExploratoryConfigError> {
        if length_hops < MIN_HOPS {
            return Err(ExploratoryConfigError::LengthTooShort {
                actual: length_hops,
                minimum: MIN_HOPS,
            });
        }
        if length_hops > MAX_HOPS {
            return Err(ExploratoryConfigError::LengthTooLong {
                actual: length_hops,
                maximum: MAX_HOPS,
            });
        }
        if max_inbound == 0 {
            return Err(ExploratoryConfigError::ZeroInbound);
        }
        if max_inbound > MAX_EXPLORATORY_INBOUND {
            return Err(ExploratoryConfigError::InboundExceedsMaximum {
                actual: max_inbound,
                maximum: MAX_EXPLORATORY_INBOUND,
            });
        }
        if max_outbound > MAX_EXPLORATORY_OUTBOUND {
            return Err(ExploratoryConfigError::OutboundExceedsMaximum {
                actual: max_outbound,
                maximum: MAX_EXPLORATORY_OUTBOUND,
            });
        }
        if build_concurrency == 0 {
            return Err(ExploratoryConfigError::ZeroBuildConcurrency);
        }
        if build_concurrency > MAX_BUILD_CONCURRENCY {
            return Err(ExploratoryConfigError::BuildConcurrencyExceedsMaximum {
                actual: build_concurrency,
                maximum: MAX_BUILD_CONCURRENCY,
            });
        }
        if failure_threshold > MAX_FAILURE_THRESHOLD {
            return Err(ExploratoryConfigError::FailureThresholdExceedsMaximum {
                actual: failure_threshold,
                maximum: MAX_FAILURE_THRESHOLD,
            });
        }
        let lifetime = match TunnelLifetime::from_seconds(lifetime_seconds) {
            Ok(value) => value,
            Err(error) => return Err(ExploratoryConfigError::Lifetime(error)),
        };
        Ok(Self {
            max_inbound,
            max_outbound,
            length_hops,
            lifetime,
            build_concurrency,
            failure_threshold,
        })
    }

    /// Maximum number of inbound exploratory tunnels.
    pub const fn max_inbound(&self) -> u16 {
        self.max_inbound
    }

    /// Maximum number of outbound exploratory tunnels.
    pub const fn max_outbound(&self) -> u16 {
        self.max_outbound
    }

    /// Tunnel hop length.
    pub const fn length_hops(&self) -> u8 {
        self.length_hops
    }

    /// Tunnel lifetime.
    pub const fn lifetime(&self) -> TunnelLifetime {
        self.lifetime
    }

    /// Maximum concurrent in-flight builds.
    pub const fn build_concurrency(&self) -> u16 {
        self.build_concurrency
    }

    /// Failure threshold above which the pool stops attempting new
    /// builds until a success is observed.
    pub const fn failure_threshold(&self) -> u16 {
        self.failure_threshold
    }
}

impl Default for ExploratoryPoolConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Validation failures for [`ExploratoryPoolConfig`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploratoryConfigError {
    /// The tunnel hop length was below the documented minimum.
    LengthTooShort {
        /// Actual supplied length.
        actual: u8,
        /// Minimum accepted length.
        minimum: u8,
    },
    /// The tunnel hop length exceeded the documented maximum.
    LengthTooLong {
        /// Actual supplied length.
        actual: u8,
        /// Maximum accepted length.
        maximum: u8,
    },
    /// The inbound ceiling was zero.
    ZeroInbound,
    /// The inbound ceiling exceeded the maximum.
    InboundExceedsMaximum {
        /// Actual supplied ceiling.
        actual: u16,
        /// Maximum accepted ceiling.
        maximum: u16,
    },
    /// The outbound ceiling exceeded the maximum.
    OutboundExceedsMaximum {
        /// Actual supplied ceiling.
        actual: u16,
        /// Maximum accepted ceiling.
        maximum: u16,
    },
    /// The build concurrency was zero.
    ZeroBuildConcurrency,
    /// The build concurrency exceeded the maximum.
    BuildConcurrencyExceedsMaximum {
        /// Actual supplied ceiling.
        actual: u16,
        /// Maximum accepted ceiling.
        maximum: u16,
    },
    /// The failure threshold exceeded the maximum.
    FailureThresholdExceedsMaximum {
        /// Actual supplied threshold.
        actual: u16,
        /// Maximum accepted threshold.
        maximum: u16,
    },
    /// The lifetime was rejected by [`TunnelLifetime::from_seconds`].
    Lifetime(crate::identity::TunnelLifetimeError),
}

impl fmt::Display for ExploratoryConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthTooShort { actual, minimum } => {
                write!(formatter, "tunnel length {actual} below minimum {minimum}")
            }
            Self::LengthTooLong { actual, maximum } => {
                write!(
                    formatter,
                    "tunnel length {actual} exceeds maximum {maximum}"
                )
            }
            Self::ZeroInbound => formatter.write_str("max inbound must be nonzero"),
            Self::InboundExceedsMaximum { actual, maximum } => {
                write!(formatter, "max inbound {actual} exceeds maximum {maximum}")
            }
            Self::OutboundExceedsMaximum { actual, maximum } => {
                write!(formatter, "max outbound {actual} exceeds maximum {maximum}")
            }
            Self::ZeroBuildConcurrency => formatter.write_str("build concurrency must be nonzero"),
            Self::BuildConcurrencyExceedsMaximum { actual, maximum } => write!(
                formatter,
                "build concurrency {actual} exceeds maximum {maximum}"
            ),
            Self::FailureThresholdExceedsMaximum { actual, maximum } => write!(
                formatter,
                "failure threshold {actual} exceeds maximum {maximum}"
            ),
            Self::Lifetime(error) => write!(formatter, "tunnel lifetime rejected: {error}"),
        }
    }
}

impl std::error::Error for ExploratoryConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_config_is_within_bounds() {
        let config = ExploratoryPoolConfig::balanced();
        assert_eq!(config.max_inbound(), 4);
        assert_eq!(config.max_outbound(), 4);
        assert_eq!(config.length_hops(), 2);
        assert_eq!(
            config.lifetime().seconds(),
            TunnelLifetime::DEFAULT_EXPLORATORY_SECONDS
        );
        assert_eq!(config.build_concurrency(), 2);
        assert_eq!(config.failure_threshold(), 8);
    }

    #[test]
    fn config_rejects_zero_inbound_and_zero_concurrency() {
        assert_eq!(
            ExploratoryPoolConfig::try_new(0, 4, 2, 600, 2, 4),
            Err(ExploratoryConfigError::ZeroInbound)
        );
        assert_eq!(
            ExploratoryPoolConfig::try_new(4, 4, 2, 600, 0, 4),
            Err(ExploratoryConfigError::ZeroBuildConcurrency)
        );
    }

    #[test]
    fn config_rejects_length_outside_bounds() {
        assert_eq!(
            ExploratoryPoolConfig::try_new(4, 4, 0, 600, 2, 4),
            Err(ExploratoryConfigError::LengthTooShort {
                actual: 0,
                minimum: MIN_HOPS,
            })
        );
        assert_eq!(
            ExploratoryPoolConfig::try_new(4, 4, MAX_HOPS + 1, 600, 2, 4),
            Err(ExploratoryConfigError::LengthTooLong {
                actual: MAX_HOPS + 1,
                maximum: MAX_HOPS,
            })
        );
    }

    #[test]
    fn config_rejects_excess_ceilings() {
        assert_eq!(
            ExploratoryPoolConfig::try_new(MAX_EXPLORATORY_INBOUND + 1, 4, 2, 600, 2, 4),
            Err(ExploratoryConfigError::InboundExceedsMaximum {
                actual: MAX_EXPLORATORY_INBOUND + 1,
                maximum: MAX_EXPLORATORY_INBOUND,
            })
        );
        assert_eq!(
            ExploratoryPoolConfig::try_new(4, MAX_EXPLORATORY_OUTBOUND + 1, 2, 600, 2, 4),
            Err(ExploratoryConfigError::OutboundExceedsMaximum {
                actual: MAX_EXPLORATORY_OUTBOUND + 1,
                maximum: MAX_EXPLORATORY_OUTBOUND,
            })
        );
        assert_eq!(
            ExploratoryPoolConfig::try_new(4, 4, 2, 600, MAX_BUILD_CONCURRENCY + 1, 4),
            Err(ExploratoryConfigError::BuildConcurrencyExceedsMaximum {
                actual: MAX_BUILD_CONCURRENCY + 1,
                maximum: MAX_BUILD_CONCURRENCY,
            })
        );
        assert_eq!(
            ExploratoryPoolConfig::try_new(4, 4, 2, 600, 2, MAX_FAILURE_THRESHOLD + 1),
            Err(ExploratoryConfigError::FailureThresholdExceedsMaximum {
                actual: MAX_FAILURE_THRESHOLD + 1,
                maximum: MAX_FAILURE_THRESHOLD,
            })
        );
    }

    #[test]
    fn config_rejects_lifetime_outside_bounds() {
        let error =
            ExploratoryPoolConfig::try_new(4, 4, 2, TunnelLifetime::MAX_LIFETIME_SECONDS + 1, 2, 4)
                .unwrap_err();
        assert!(matches!(
            error,
            ExploratoryConfigError::Lifetime(
                crate::identity::TunnelLifetimeError::ExceedsMaximum { .. }
            )
        ));
    }
}
