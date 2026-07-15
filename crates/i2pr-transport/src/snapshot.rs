//! Privacy-safe bounded transport observations.

use std::fmt;
use std::time::Duration;

use i2pr_core::{ResourceError, ResourceUsage};

use crate::{
    AddressFamily, AddressOrigin, Confidence, Direction, LinkId, LinkState, Reachability,
    TerminationCategory, TransportKind, ValidationState,
};

/// Resource categories held by one link owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkResourceUsage {
    /// Whether this link holds one `ActiveLinks` grant.
    pub active_link_units: u64,
    /// Queue item grants held by this link.
    pub queued_message_units: u64,
    /// Buffered-byte grants held by this link.
    pub buffered_byte_units: u64,
}

/// A redacted observation of one live link.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkSnapshot {
    /// Process-local link instance identifier.
    pub link_id: LinkId,
    /// Transport kind for this link.
    pub transport: TransportKind,
    /// Inbound or outbound direction.
    pub direction: Direction,
    /// Current finite lifecycle state.
    pub lifecycle: LinkState,
    /// Whether authentication has completed for this link instance.
    pub authenticated: bool,
    /// Number of queued messages owned by this link.
    pub queued_messages: u64,
    /// Number of queued bytes owned by this link.
    pub queued_bytes: u64,
    /// Bounded monotonic age of this link instance.
    pub age: Duration,
    /// Last typed termination category, if a manager recorded one.
    pub last_termination: Option<TerminationCategory>,
    /// Resource categories and exact bounded counters held by this link.
    pub resources: LinkResourceUsage,
}

/// A transport-neutral address/reachability observation with no raw endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReachabilityObservation {
    /// Transport that produced the observation.
    pub transport: TransportKind,
    /// Whether the source was configured or observed.
    pub origin: AddressOrigin,
    /// Coarse address family category.
    pub family: AddressFamily,
    /// Coarse reachability category.
    pub reachability: Reachability,
    /// Monotonic timestamp supplied by the owner.
    pub observed_at: Duration,
    /// Bounded local validation state.
    pub validation: ValidationState,
    /// Optional bounded confidence score.
    pub confidence: Option<Confidence>,
}

/// A bounded aggregate of transport state and resource usage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportSnapshot {
    /// Live link entries in local identifier order.
    pub links: Vec<LinkSnapshot>,
    /// Recent typed observations in deterministic order.
    pub observations: Vec<ReachabilityObservation>,
    /// Shared resource usage in core-class order.
    pub resources: Vec<ResourceUsage>,
    /// Number of peers with recorded dial/backoff state, without peer labels.
    pub dial_backoff_entries: usize,
}

/// Snapshot construction or resource-accounting failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// The shared budget could not produce a snapshot.
    Resource(ResourceError),
    /// The manager supplied more link entries than the privacy bound permits.
    TooManyLinks {
        /// Snapshot entry maximum.
        maximum: usize,
    },
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(error) => error.fmt(formatter),
            Self::TooManyLinks { maximum } => {
                write!(formatter, "transport snapshot exceeds {maximum} links")
            }
        }
    }
}

impl std::error::Error for SnapshotError {}
