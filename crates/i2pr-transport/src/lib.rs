//! Runtime-neutral ownership and delivery contracts for router transports.
//!
//! This crate contains bounded values and synchronous state decisions only.
//! It does not own a runtime, sockets, timers, NetDB state, tunnel state, or
//! client delivery. [`TransportManager`] is driven by an owning runtime
//! service through explicit method calls; it never waits or spawns work.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use i2pr_core::{
    ResourceBudget, ResourceBundle, ResourceClass, ResourceError, ResourceLease, ResourceLimit,
    ResourceRequest, ResourceUsage,
};

mod delivery;
mod identity;
mod lifecycle;
mod manager;
mod payload;
mod resource;
mod snapshot;
mod types;

#[cfg(test)]
mod tests;

pub use delivery::{DeliveryOutcome, DeliveryRequest, QueuedDelivery};
pub use identity::PeerId;
pub use lifecycle::{InvalidLinkTransition, LinkState};
pub use manager::{
    CandidateAdmissionError, CandidateDecision, CloseOutcome, DialBackoff, DialBackoffError,
    DuplicateLinkPolicy, DuplicateResolution, LinkCandidate, LinkDeliveryCapability,
    PendingHandshake, ReachabilityRecordOutcome, RegistrationError, RegistrationOutcome,
    RegistrationRejection, TransportManager,
};
pub use payload::{EncodedI2npMessage, PayloadError};
pub use resource::{
    TransportLease, TransportLimits, TransportQueueLease, TransportResourceLimitsError,
    TransportResources,
};
pub use snapshot::{
    LinkResourceUsage, LinkSnapshot, ReachabilityObservation, SnapshotError, TransportSnapshot,
};
/// Compatibility name for a transport-neutral reachability observation.
pub type AddressObservation = ReachabilityObservation;
pub use types::{
    AddressFamily, AddressOrigin, Confidence, ConfidenceError, Deadline, DeadlineError, DeliveryId,
    Direction, LinkDirection, LinkId, LinkIdError, MAX_DEADLINE, MAX_I2NP_MESSAGE_BYTES,
    MAX_LINK_ID, MAX_LINK_SNAPSHOT_ENTRIES, MAX_REACHABILITY_OBSERVATIONS, Reachability,
    TerminationCategory, TransportKind, ValidationState,
};
