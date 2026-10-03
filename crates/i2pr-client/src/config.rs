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
/// Proposal 170 backup-quantity ceiling (Plan 296): 0..=3
/// standby tunnels held ready per direction beyond the quantity
/// target.
pub const MAX_DESTINATION_BACKUP_QUANTITY: u8 = 3;
/// Proposal 170 length-variance bound (Plan 296): per-build hop
/// adjustment sampled from `-variance..=+variance` (a radius; the
/// sign carries no direction and is ignored by the sampler).
pub const MAX_DESTINATION_LENGTH_VARIANCE: i8 = 2;
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

/// Explicit destination tunnel mode (Plan 172 §8).
///
/// Remote mode preserves the existing one-plus-hop tunnel policy.
/// LocalZeroHop mode is the legitimate localhost path where gateway ==
/// endpoint == this router, with no remote hops and no LayerKeys.
/// Mixed inbound/outbound modes are rejected explicitly by the I2CP
/// projection; the destination pool never silently rewrites them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationTunnelMode {
    /// Remote established tunnels with `length_hops >= 1`.
    Remote {
        /// Tunnel hop length.
        length_hops: u8,
    },
    /// Local zero-hop path (length 0 + allowZeroHop).
    LocalZeroHop,
}

impl DestinationTunnelMode {
    /// Whether the mode is the local zero-hop path.
    pub const fn is_local_zero_hop(self) -> bool {
        matches!(self, Self::LocalZeroHop)
    }
}

/// Runtime-neutral local-router context for zero-hop composition
/// (Plan 172 §7).
///
/// The hash must be the actual configured router identity hash of the
/// process that owns the I2CP service (an ephemeral test-process
/// identity is acceptable for localhost, but it must remain stable
/// for the session). No private key material is carried here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalRouterContext {
    /// Local router hash used as the zero-hop Lease gateway.
    pub local_router_hash: i2pr_proto::Hash,
}

impl LocalRouterContext {
    /// Constructs a local-router context from the supplied hash.
    /// Rejects the all-zero hash.
    pub fn new(local_router_hash: i2pr_proto::Hash) -> Result<Self, DestinationConfigError> {
        if local_router_hash == i2pr_proto::Hash::from_bytes([0; 32]) {
            return Err(DestinationConfigError::ZeroInboundTarget);
        }
        Ok(Self { local_router_hash })
    }
}

/// Bounded per-destination configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestinationConfig {
    inbound_target: u16,
    outbound_target: u16,
    minimum_usable_inbound: u16,
    length_hops: u8,
    backup_quantity: u8,
    length_variance: i8,
    reply_bundling: bool,
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
            0,
            0,
            false,
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

    /// Builds a configuration from validated service-tunnel shaping.
    ///
    /// Plan 292 projection: quantities drive the inbound/outbound pool
    /// targets and the single shared hop length drives both directions
    /// (mirroring the I2CP first-wins rule; the control boundary rejects
    /// differing per-direction lengths). Plan 296 projection: the
    /// backup quantity raises each direction's effective pool target
    /// (standby tunnels held ready; usability still keys on the base
    /// target) and the length variance is carried for the build
    /// sampler. Every other field keeps the [`Self::balanced`]
    /// default, and `minimum_usable_inbound` keeps the I2CP precedent
    /// (`min(inbound, balanced minimum)` = 1), so `(2, 2, 2, 0, 0)`
    /// reproduces [`Self::balanced`] exactly. Caller ceilings
    /// (1..=6 quantities, 1..=3 length, 0..=3 backup, -2..=+2
    /// variance, per-direction quantity plus backup within the pool
    /// maximum) sit inside the destination ceilings, hence the expect.
    pub fn from_service_shaping(
        inbound_quantity: u8,
        outbound_quantity: u8,
        length_hops: u8,
        backup_quantity: u8,
        length_variance: i8,
    ) -> Self {
        Self::try_new(
            u16::from(inbound_quantity),
            u16::from(outbound_quantity),
            1,
            length_hops,
            backup_quantity,
            length_variance,
            false,
            TunnelLifetime::DEFAULT_EXPLORATORY_SECONDS,
            2,
            8,
            64,
            128 * 1024,
            DEFAULT_LEASE_PUBLICATION_MARGIN_SECONDS,
            DEFAULT_LEASE_ROTATION_MARGIN_SECONDS,
        )
        .expect("service shaping ceilings sit inside destination ceilings")
    }

    /// Returns a copy with the garlic reply-bundling delivery
    /// policy set (Plan 296). The pool sizing is untouched; the
    /// outbound delivery path consults the flag when batching
    /// same-remote replies.
    pub const fn with_reply_bundling(mut self, reply_bundling: bool) -> Self {
        self.reply_bundling = reply_bundling;
        self
    }

    /// Builds a configuration after applying every documented ceiling.
    #[allow(clippy::too_many_arguments)]
    pub const fn try_new(
        inbound_target: u16,
        outbound_target: u16,
        minimum_usable_inbound: u16,
        length_hops: u8,
        backup_quantity: u8,
        length_variance: i8,
        reply_bundling: bool,
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
        if backup_quantity > MAX_DESTINATION_BACKUP_QUANTITY {
            return Err(DestinationConfigError::BackupExceedsMaximum {
                actual: backup_quantity,
                maximum: MAX_DESTINATION_BACKUP_QUANTITY,
            });
        }
        // `contains` is not const-compatible; the explicit
        // comparisons keep `try_new` const.
        #[allow(clippy::manual_range_contains)]
        if length_variance < -MAX_DESTINATION_LENGTH_VARIANCE
            || length_variance > MAX_DESTINATION_LENGTH_VARIANCE
        {
            return Err(DestinationConfigError::VarianceOutOfRange {
                actual: length_variance,
            });
        }
        // Standby tunnels ride the same pool, so each direction's
        // base target plus backup must fit the directional maximum.
        // The pool projection below reuses these sums as its
        // maximums; rejecting here (never clamping) keeps the
        // control plane from silently dropping requested standby.
        if inbound_target.saturating_add(backup_quantity as u16) > MAX_DESTINATION_INBOUND {
            return Err(DestinationConfigError::StandbyExceedsMaximum {
                direction: "inbound",
                effective: inbound_target.saturating_add(backup_quantity as u16),
                maximum: MAX_DESTINATION_INBOUND,
            });
        }
        if outbound_target.saturating_add(backup_quantity as u16) > MAX_DESTINATION_OUTBOUND {
            return Err(DestinationConfigError::StandbyExceedsMaximum {
                direction: "outbound",
                effective: outbound_target.saturating_add(backup_quantity as u16),
                maximum: MAX_DESTINATION_OUTBOUND,
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
            backup_quantity,
            length_variance,
            reply_bundling,
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

    /// Standby tunnels held ready per direction beyond the base
    /// quantity target (Plan 296).
    pub const fn backup_quantity(&self) -> u8 {
        self.backup_quantity
    }

    /// Per-build hop-length variance radius around
    /// [`Self::length_hops`] (Plan 296).
    pub const fn length_variance(&self) -> i8 {
        self.length_variance
    }

    /// Whether the outbound delivery path may bundle multiple
    /// same-remote application payloads into one New Session Reply
    /// garlic message (Plan 296).
    pub const fn reply_bundling(&self) -> bool {
        self.reply_bundling
    }

    /// Effective inbound pool target: base quantity plus standby.
    /// The constructor guarantees the sum fits the directional
    /// maximum, so the saturating add below never clamps.
    pub const fn effective_inbound_target(&self) -> u16 {
        // `saturating_add` in const context; the ceiling check in
        // `try_new` keeps the sum exact.
        if self.inbound_target + self.backup_quantity as u16 > MAX_DESTINATION_INBOUND {
            return MAX_DESTINATION_INBOUND;
        }
        self.inbound_target + self.backup_quantity as u16
    }

    /// Effective outbound pool target: base quantity plus standby.
    pub const fn effective_outbound_target(&self) -> u16 {
        if self.outbound_target + self.backup_quantity as u16 > MAX_DESTINATION_OUTBOUND {
            return MAX_DESTINATION_OUTBOUND;
        }
        self.outbound_target + self.backup_quantity as u16
    }

    /// Samples one build's hop length around `base_hops` within
    /// `-variance..=+variance` (Plan 296).
    ///
    /// The variance is a radius: the sign carries no direction, so
    /// `-2` and `+2` sample the same five-point distribution. The
    /// result is floored and ceiled at the pool hop policy
    /// (`i2pr_tunnel` `MIN_HOPS..=MAX_HOPS`). `sample` is
    /// caller-supplied randomness: production passes CSPRNG bytes
    /// and unit paths inject fixed bytes, so this function never
    /// touches ambient RNG.
    pub fn sampled_build_length(base_hops: u8, variance: i8, sample: u8) -> u8 {
        use i2pr_tunnel::config::{MAX_HOPS, MIN_HOPS};
        let radius = variance.unsigned_abs().min(MAX_HOPS);
        if radius == 0 {
            return base_hops.clamp(MIN_HOPS, MAX_HOPS);
        }
        let width = u16::from(radius) * 2 + 1;
        let offset = (u16::from(sample) % width) as i16 - i16::from(radius);
        (i16::from(base_hops) + offset).clamp(i16::from(MIN_HOPS), i16::from(MAX_HOPS)) as u8
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
    ///
    /// Plan 296: the pool maximums are the effective targets (base
    /// quantity plus standby), so standby tunnels register in the
    /// same bounded container while usability still keys on the
    /// base target. The constructor guarantees the sums fit the
    /// directional ceilings.
    pub const fn pool_config(&self) -> Result<BoundedTunnelPoolConfig, ExploratoryConfigError> {
        BoundedTunnelPoolConfig::try_new(
            self.effective_inbound_target(),
            self.effective_outbound_target(),
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
    /// The standby quantity exceeded the Proposal ceiling (Plan 296).
    BackupExceedsMaximum {
        /// Supplied backup quantity.
        actual: u8,
        /// Accepted ceiling.
        maximum: u8,
    },
    /// The length variance left the Proposal bound (Plan 296).
    VarianceOutOfRange {
        /// Supplied variance.
        actual: i8,
    },
    /// A direction's base target plus standby exceeded the
    /// directional pool maximum (Plan 296; never clamped).
    StandbyExceedsMaximum {
        /// Direction whose effective target overflowed.
        direction: &'static str,
        /// Requested base plus backup.
        effective: u16,
        /// Accepted ceiling.
        maximum: u16,
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
            Self::BackupExceedsMaximum { actual, maximum } => write!(
                formatter,
                "backup quantity {actual} exceeds maximum {maximum}"
            ),
            Self::VarianceOutOfRange { actual } => {
                write!(formatter, "length variance {actual} is outside -2..=+2")
            }
            Self::StandbyExceedsMaximum {
                direction,
                effective,
                maximum,
            } => write!(
                formatter,
                "{direction} quantity plus backup {effective} exceeds maximum {maximum}"
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
    fn service_shaping_projection_matches_balanced_default() {
        // Plan 292: (2, 2, 2) reproduces balanced exactly; other
        // in-bounds shaping projects quantity/length truthfully.
        // Plan 296: zero backup/variance reproduces balanced.
        assert_eq!(
            DestinationConfig::from_service_shaping(2, 2, 2, 0, 0),
            DestinationConfig::balanced()
        );
        let shaped = DestinationConfig::from_service_shaping(4, 1, 3, 0, 0);
        assert_eq!(shaped.inbound_target(), 4);
        assert_eq!(shaped.outbound_target(), 1);
        assert_eq!(shaped.minimum_usable_inbound(), 1);
        assert_eq!(shaped.length_hops(), 3);
        assert!(shaped.pool_config().is_ok());
    }

    #[test]
    fn service_shaping_projection_carries_standby_and_variance() {
        // Plan 296: backup raises the effective pool targets while
        // usability keys on the base target; variance is carried
        // for the build sampler; bundling defaults off.
        let shaped = DestinationConfig::from_service_shaping(2, 3, 2, 2, -1);
        assert_eq!(shaped.inbound_target(), 2);
        assert_eq!(shaped.outbound_target(), 3);
        assert_eq!(shaped.backup_quantity(), 2);
        assert_eq!(shaped.length_variance(), -1);
        assert!(!shaped.reply_bundling());
        assert_eq!(shaped.effective_inbound_target(), 4);
        assert_eq!(shaped.effective_outbound_target(), 5);
        assert!(shaped.pool_config().is_ok());
        let bundled = shaped.with_reply_bundling(true);
        assert!(bundled.reply_bundling());
        assert_eq!(bundled.effective_inbound_target(), 4);
    }

    #[test]
    fn interactive_streaming_profile_constrains_windows() {
        // Plan 292: the interactive profile keeps bulk timeouts and
        // stream ceilings but constrains windows and the ACK
        // deadline for responsiveness.
        use crate::streaming::config::StreamingConfig;
        let interactive = StreamingConfig::interactive();
        let balanced = StreamingConfig::balanced();
        assert_eq!(interactive.max_send_window_packets, 16);
        assert_eq!(interactive.max_recv_window_packets, 16);
        assert_eq!(interactive.max_unacked_packets, 32);
        assert_eq!(interactive.delayed_ack_ms, 100);
        assert_eq!(
            interactive.max_streams_per_destination,
            balanced.max_streams_per_destination
        );
        assert_eq!(interactive.setup_timeout_ms, balanced.setup_timeout_ms);
        assert_eq!(interactive.close_timeout_ms, balanced.close_timeout_ms);
    }

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
            DestinationConfig::try_new(0, 2, 1, 2, 0, 0, false, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::ZeroInboundTarget)
        );
        assert_eq!(
            DestinationConfig::try_new(
                MAX_DESTINATION_INBOUND + 1,
                2,
                1,
                2,
                0,
                0,
                false,
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
            DestinationConfig::try_new(2, 2, 3, 2, 0, 0, false, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::MinimumUsableExceedsTarget {
                minimum: 3,
                target: 2,
            })
        );
        assert_eq!(
            DestinationConfig::try_new(2, 2, 1, 2, 0, 0, false, 600, 2, 8, 0, 1024, 60, 120),
            Err(DestinationConfigError::ZeroPendingMessages)
        );
        assert_eq!(
            DestinationConfig::try_new(2, 2, 1, 2, 0, 0, false, 60, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::PublicationMarginExceedsLifetime {
                margin: 60,
                lifetime: 60,
            })
        );
        // The balanced configuration remains untouched by rejected attempts.
        assert_eq!(base, DestinationConfig::balanced());
    }

    #[test]
    fn configuration_rejects_standby_and_variance_excess() {
        // Plan 296: backup binds 0..=3, variance binds -2..=+2,
        // and neither direction's base plus backup may exceed the
        // directional pool maximum (never clamped).
        assert_eq!(
            DestinationConfig::try_new(2, 2, 1, 2, 4, 0, false, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::BackupExceedsMaximum {
                actual: 4,
                maximum: MAX_DESTINATION_BACKUP_QUANTITY,
            })
        );
        assert_eq!(
            DestinationConfig::try_new(2, 2, 1, 2, 0, 3, false, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::VarianceOutOfRange { actual: 3 })
        );
        assert_eq!(
            DestinationConfig::try_new(2, 2, 1, 2, 0, -3, false, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::VarianceOutOfRange { actual: -3 })
        );
        assert_eq!(
            DestinationConfig::try_new(6, 2, 1, 2, 3, 0, false, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::StandbyExceedsMaximum {
                direction: "inbound",
                effective: 9,
                maximum: MAX_DESTINATION_INBOUND,
            })
        );
        assert_eq!(
            DestinationConfig::try_new(2, 6, 1, 2, 3, 0, false, 600, 2, 8, 64, 1024, 60, 120),
            Err(DestinationConfigError::StandbyExceedsMaximum {
                direction: "outbound",
                effective: 9,
                maximum: MAX_DESTINATION_OUTBOUND,
            })
        );
        // Boundary sums fit exactly.
        let edge = DestinationConfig::try_new(6, 5, 1, 2, 2, 2, true, 600, 2, 8, 64, 1024, 60, 120)
            .expect("boundary standby fits");
        assert_eq!(edge.effective_inbound_target(), 8);
        assert_eq!(edge.effective_outbound_target(), 7);
        assert!(edge.pool_config().is_ok());
    }

    #[test]
    fn sampled_build_length_covers_variance_window() {
        // Plan 296: zero variance reproduces the base; nonzero
        // variance samples uniformly across the window with
        // injected bytes (never ambient RNG); the pool hop policy
        // floors and ceils the result; the sign is a radius.
        for sample in 0..=u8::MAX {
            assert_eq!(DestinationConfig::sampled_build_length(2, 0, sample), 2);
        }
        let mut seen = [false; 6];
        for sample in 0..=u8::MAX {
            let length = DestinationConfig::sampled_build_length(3, 2, sample);
            assert!((1..=5).contains(&length), "variance window 1..=5");
            seen[usize::from(length)] = true;
        }
        assert!(
            seen[1..=5].iter().all(|hit| *hit),
            "every window point sampled"
        );
        // Negative variance samples the identical window.
        for sample in 0..=u8::MAX {
            assert_eq!(
                DestinationConfig::sampled_build_length(3, -2, sample),
                DestinationConfig::sampled_build_length(3, 2, sample)
            );
        }
        // Pool policy floors at 1 and ceils at 8.
        assert_eq!(DestinationConfig::sampled_build_length(1, 2, 0), 1);
        assert_eq!(DestinationConfig::sampled_build_length(8, 2, 4), 8);
        assert_eq!(DestinationConfig::sampled_build_length(3, 1, 0), 2);
        assert_eq!(DestinationConfig::sampled_build_length(3, 1, 1), 3);
        assert_eq!(DestinationConfig::sampled_build_length(3, 1, 2), 4);
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
