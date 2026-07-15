//! Small bounded vocabulary shared by transport contracts.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Largest local identifier accepted by the transport contracts.
pub const MAX_LINK_ID: u64 = 0x7fff_ffff_ffff_ffff;
/// Largest encoded I2NP message accepted by the transport boundary.
pub const MAX_I2NP_MESSAGE_BYTES: usize =
    i2pr_proto::MAX_I2NP_PAYLOAD_SIZE + i2pr_proto::STANDARD_HEADER_SIZE;
/// Maximum absolute monotonic deadline retained by a delivery request.
pub const MAX_DEADLINE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Maximum number of link entries retained in a manager snapshot.
pub const MAX_LINK_SNAPSHOT_ENTRIES: usize = 256;
/// Maximum number of address observations retained by one manager.
pub const MAX_REACHABILITY_OBSERVATIONS: usize = 64;

static NEXT_LINK_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_DELIVERY_ID: AtomicU64 = AtomicU64::new(1);

/// Transport implementations currently covered by the contract surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TransportKind {
    /// The NTCP2 stream transport; wire behavior is deferred to later plans.
    Ntcp2,
}

/// Direction of a transport link relative to this router.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Direction {
    /// A remote peer established the link to this router.
    Inbound,
    /// This router initiated the link.
    Outbound,
}

/// Compatibility name for a link's direction at transport boundaries.
pub type LinkDirection = Direction;

/// Locally generated identifier for one link instance.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkId(u64);

impl LinkId {
    /// Validates a caller-supplied local identifier.
    pub const fn new(value: u64) -> Result<Self, LinkIdError> {
        if value == 0 {
            Err(LinkIdError::Zero)
        } else if value > MAX_LINK_ID {
            Err(LinkIdError::TooLarge)
        } else {
            Ok(Self(value))
        }
    }

    /// Generates a process-local identifier that is not derived from a peer.
    pub fn generate() -> Result<Self, LinkIdError> {
        NEXT_LINK_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                (current < MAX_LINK_ID).then_some(current + 1)
            })
            .map(Self)
            .map_err(|_| LinkIdError::Exhausted)
    }

    /// Returns the opaque local counter value for internal correlation.
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for LinkId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("LinkId").field(&self.0).finish()
    }
}

/// Validation failures for a local link identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkIdError {
    /// Zero is reserved as an invalid sentinel.
    Zero,
    /// The value exceeds [`MAX_LINK_ID`].
    TooLarge,
    /// The process-local identifier space is exhausted.
    Exhausted,
}

impl fmt::Display for LinkIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("link identifier must be nonzero"),
            Self::TooLarge => formatter.write_str("link identifier exceeds its bound"),
            Self::Exhausted => formatter.write_str("link identifier space is exhausted"),
        }
    }
}

impl std::error::Error for LinkIdError {}

/// Opaque identifier for a delivery response path owned by the runtime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeliveryId(u64);

impl DeliveryId {
    /// Generates a process-local delivery identifier.
    pub fn generate() -> Result<Self, LinkIdError> {
        NEXT_DELIVERY_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                (current < MAX_LINK_ID).then_some(current + 1)
            })
            .map(Self)
            .map_err(|_| LinkIdError::Exhausted)
    }

    /// Returns the opaque identifier for a runtime-owned response map.
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// An absolute time on the monotonic clock supplied by the owning runtime.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Deadline(Duration);

impl Deadline {
    /// Validates an absolute monotonic deadline.
    pub fn new(at: Duration) -> Result<Self, DeadlineError> {
        if at > MAX_DEADLINE {
            Err(DeadlineError::TooFar)
        } else {
            Ok(Self(at))
        }
    }

    /// Returns the absolute monotonic time represented by this deadline.
    pub const fn at(self) -> Duration {
        self.0
    }

    /// Returns whether the deadline has elapsed at `now`.
    pub fn is_elapsed(self, now: Duration) -> bool {
        now >= self.0
    }

    /// Returns the nonnegative time remaining at `now`.
    pub const fn remaining(self, now: Duration) -> Duration {
        self.0.saturating_sub(now)
    }
}

/// Validation failures for a delivery deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeadlineError {
    /// The absolute time exceeds [`MAX_DEADLINE`].
    TooFar,
}

impl fmt::Display for DeadlineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("deadline exceeds its monotonic bound")
    }
}

impl std::error::Error for DeadlineError {}

/// Typed origin of a reachability observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AddressOrigin {
    /// The observation came from locally configured metadata.
    Configured,
    /// The observation came from a transport exchange or local socket layer.
    Observed,
}

/// Address family category without an address or port value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AddressFamily {
    /// An IPv4 address was classified.
    Ipv4,
    /// An IPv6 address was classified.
    Ipv6,
    /// The source could not classify an address family.
    Unknown,
}

/// Coarse reachability state retained by transport-neutral observations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Reachability {
    /// No reachability result is available.
    Unknown,
    /// The observation was locally checked but not confirmed reachable.
    Unconfirmed,
    /// The observation was confirmed by a bounded local test.
    Reachable,
    /// The observation was locally rejected as unreachable.
    Unreachable,
}

/// Validation stage attached to a reachability observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ValidationState {
    /// No validation has been performed.
    Unvalidated,
    /// The bounded observation passed its local validation.
    Validated,
    /// The bounded observation failed local validation.
    Rejected,
}

/// A confidence score bounded to the inclusive range 0..=100.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Confidence(u8);

impl Confidence {
    /// Creates a confidence score in the inclusive percentage range.
    pub const fn new(value: u8) -> Result<Self, ConfidenceError> {
        if value > 100 {
            Err(ConfidenceError::TooLarge)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the bounded percentage value.
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// Validation failures for a confidence score.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfidenceError {
    /// The score exceeded 100 percent.
    TooLarge,
}

impl fmt::Display for ConfidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("confidence must be at most 100")
    }
}

impl std::error::Error for ConfidenceError {}

/// Fixed termination categories safe for aggregate diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerminationCategory {
    /// The local router requested shutdown.
    LocalShutdown,
    /// The remote peer terminated the authenticated link.
    RemoteTermination,
    /// Authentication evidence was rejected.
    AuthenticationFailure,
    /// A bounded handshake or delivery deadline elapsed.
    Timeout,
    /// Replay or clock-skew policy rejected an input.
    ReplayOrSkewRejection,
    /// A frame or block was structurally malformed.
    MalformedFraming,
    /// A bounded queue could not accept another item.
    QueueExhaustion,
    /// A shared resource budget denied an admission.
    ResourceExhaustion,
    /// A replacement policy retired the previous link.
    DuplicateReplacement,
    /// The underlying I/O owner closed the link.
    IoClosure,
}
