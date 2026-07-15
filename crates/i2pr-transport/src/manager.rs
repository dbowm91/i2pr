//! Synchronous transport-manager decisions and exact ownership accounting.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use i2pr_core::{ResourceClass, ResourceError};

use crate::delivery::{DeliveryOutcome, QueuedDelivery};
use crate::identity::PeerId;
use crate::lifecycle::LinkState;
use crate::resource::{TransportLease, TransportLimits, TransportResources};
use crate::snapshot::{
    LinkResourceUsage, LinkSnapshot, ReachabilityObservation, SnapshotError, TransportSnapshot,
};
use crate::types::{
    Direction, LinkId, MAX_DEADLINE, MAX_LINK_SNAPSHOT_ENTRIES, MAX_REACHABILITY_OBSERVATIONS,
    TerminationCategory, TransportKind,
};
use crate::{DeliveryRequest, LinkIdError};

/// The duplicate policy supplied by a later runtime or routing-policy owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateResolution {
    /// Keep both links if the peer and global limits permit it.
    AcceptNew,
    /// Replace the existing link with the candidate.
    ReplaceExisting,
    /// Reject the candidate and keep the existing link.
    RejectNew,
    /// Keep the existing link while the candidate drains.
    RetainExistingDrainNew,
}

/// Selects a deterministic winner for simultaneous inbound/outbound links.
///
/// The runtime supplies the local router reference because this crate does not
/// own router identity or policy.  When the two directions race, the side
/// whose direction is preferred by the local/remote hash ordering wins.  A
/// candidate that wins replaces the current link; a losing candidate is
/// retained only long enough for its runtime owner to drain and close it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DuplicateLinkPolicy {
    local_peer: PeerId,
}

impl DuplicateLinkPolicy {
    /// Creates the policy for one local router reference.
    pub const fn new(local_peer: PeerId) -> Self {
        Self { local_peer }
    }

    /// Returns the bounded duplicate decision for an existing/candidate pair.
    pub fn decide(
        self,
        existing: &LinkCandidate,
        candidate: &LinkCandidate,
    ) -> DuplicateResolution {
        if existing.peer() != candidate.peer() {
            return DuplicateResolution::AcceptNew;
        }
        if existing.direction() == candidate.direction() {
            return DuplicateResolution::RejectNew;
        }
        let outbound_wins = self.local_peer < candidate.peer();
        let candidate_wins = (outbound_wins && candidate.direction() == Direction::Outbound)
            || (!outbound_wins && candidate.direction() == Direction::Inbound);
        if candidate_wins {
            DuplicateResolution::ReplaceExisting
        } else {
            DuplicateResolution::RetainExistingDrainNew
        }
    }
}

/// A bounded candidate-admission failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationError {
    /// Manager state was poisoned by an earlier panic.
    StatePoisoned,
    /// The shared resource budget rejected an exact lease.
    Resource(ResourceError),
    /// The requested link was not present.
    MissingLink,
    /// A link ID is already owned by another candidate.
    DuplicateLinkId,
    /// A lifecycle transition was invalid.
    InvalidTransition(crate::InvalidLinkTransition),
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StatePoisoned => formatter.write_str("transport manager state is poisoned"),
            Self::Resource(error) => error.fmt(formatter),
            Self::MissingLink => formatter.write_str("transport link is not present"),
            Self::DuplicateLinkId => {
                formatter.write_str("transport link identifier is already owned")
            }
            Self::InvalidTransition(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RegistrationError {}

impl From<ResourceError> for RegistrationError {
    fn from(error: ResourceError) -> Self {
        Self::Resource(error)
    }
}

impl From<crate::InvalidLinkTransition> for RegistrationError {
    fn from(error: crate::InvalidLinkTransition) -> Self {
        Self::InvalidTransition(error)
    }
}

/// Compatibility name for errors from pending-handshake admission.
pub type CandidateAdmissionError = RegistrationError;

/// The manager's complete candidate decision surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateDecision {
    /// The first link for a peer was accepted.
    AcceptFirst {
        /// The newly owned local link identifier.
        link_id: LinkId,
    },
    /// An additional link was accepted within the peer limit.
    AcceptAdditional {
        /// The newly owned local link identifier.
        link_id: LinkId,
    },
    /// The existing link was replaced by the candidate.
    ReplaceExisting {
        /// The link owner that was removed.
        existing: LinkId,
        /// The candidate that now owns the peer slot.
        candidate: LinkId,
    },
    /// The candidate was rejected as a duplicate.
    RejectNewDuplicate {
        /// The link retained by the peer slot.
        existing: LinkId,
    },
    /// The existing link stays while the candidate drains.
    RetainExistingDrainNew {
        /// The link retained by the peer slot.
        existing: LinkId,
        /// The candidate that must drain outside the active set.
        candidate: LinkId,
    },
    /// The peer-scoped active-link limit was reached.
    RejectPeerLimit {
        /// The configured per-peer ceiling.
        maximum: u64,
    },
    /// The global active-link limit was reached.
    RejectGlobalLimit {
        /// The configured global active-link ceiling.
        maximum: u64,
    },
    /// Authentication evidence was incomplete.
    RejectIncompleteAuthentication,
    /// The active-link lease could not be admitted.
    RejectResourceDenied,
    /// Pending and candidate peer references did not match.
    RejectPeerIdentityMismatch,
}

/// Alias naming a completed candidate-registration decision.
pub type RegistrationOutcome = CandidateDecision;

/// Rejections represented separately for callers that only need failure classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationRejection {
    /// A duplicate candidate was not selected.
    Duplicate,
    /// The peer limit was reached.
    PeerLimit,
    /// The global limit was reached.
    GlobalLimit,
    /// Authentication was incomplete.
    IncompleteAuthentication,
    /// A resource lease was denied.
    ResourceDenied,
    /// Pending and candidate peer references differed.
    PeerIdentityMismatch,
}

/// Result of removing a link owner exactly once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseOutcome {
    /// A present link was removed and its active lease released.
    Closed {
        /// The link owner that was removed.
        link_id: LinkId,
        /// State observed immediately before removal.
        previous: LinkState,
        /// Typed reason supplied by the owner.
        reason: TerminationCategory,
    },
    /// A stale close report did not affect a replacement link.
    Stale {
        /// The already-removed or replaced link identifier.
        link_id: LinkId,
    },
}

/// A bounded dial/backoff decision recorded without performing a wait.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialBackoff {
    /// Monotonic time before which a retry must not be started.
    pub retry_at: Duration,
    /// Bounded retry count retained by the owner.
    pub attempts: u16,
}

impl DialBackoff {
    /// Creates a bounded backoff record.
    pub fn new(retry_at: Duration, attempts: u16) -> Result<Self, DialBackoffError> {
        if retry_at > MAX_DEADLINE {
            Err(DialBackoffError::TooFar)
        } else {
            Ok(Self { retry_at, attempts })
        }
    }
}

/// Errors returned while constructing a dial/backoff decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialBackoffError {
    /// The monotonic retry time exceeds the bounded observation horizon.
    TooFar,
}

impl fmt::Display for DialBackoffError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("dial backoff time exceeds its monotonic bound")
    }
}

impl std::error::Error for DialBackoffError {}

/// Result of retaining one bounded reachability observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReachabilityRecordOutcome {
    /// The observation was retained without eviction.
    Retained,
    /// The oldest observation was evicted to keep the bound.
    EvictedOldest,
}

/// A candidate awaiting authenticated-link registration.
pub struct LinkCandidate {
    link_id: LinkId,
    peer: PeerId,
    transport: TransportKind,
    direction: Direction,
    state: LinkState,
}

impl LinkCandidate {
    /// Allocates a locally generated candidate in the Candidate state.
    pub fn new(
        peer: PeerId,
        transport: TransportKind,
        direction: Direction,
    ) -> Result<Self, LinkIdError> {
        Ok(Self::with_id(
            LinkId::generate()?,
            peer,
            transport,
            direction,
        ))
    }

    /// Creates a candidate with an explicitly bounded local link ID.
    pub const fn with_id(
        link_id: LinkId,
        peer: PeerId,
        transport: TransportKind,
        direction: Direction,
    ) -> Self {
        Self {
            link_id,
            peer,
            transport,
            direction,
            state: LinkState::Candidate,
        }
    }

    /// Advances this candidate into handshake state.
    pub fn begin_handshake(&mut self) -> Result<(), crate::InvalidLinkTransition> {
        self.state = self.state.transition(LinkState::Handshaking)?;
        Ok(())
    }

    /// Marks external authentication evidence as complete.
    pub fn authenticate(&mut self) -> Result<(), crate::InvalidLinkTransition> {
        self.state = self.state.transition(LinkState::Authenticated)?;
        Ok(())
    }

    /// Returns the local link ID.
    pub const fn link_id(&self) -> LinkId {
        self.link_id
    }

    /// Returns the transport peer reference.
    pub const fn peer(&self) -> PeerId {
        self.peer
    }

    /// Returns the selected transport kind.
    pub const fn transport(&self) -> TransportKind {
        self.transport
    }

    /// Returns the link direction.
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// Returns the current candidate state.
    pub const fn state(&self) -> LinkState {
        self.state
    }
}

impl fmt::Debug for LinkCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinkCandidate")
            .field("link_id", &self.link_id)
            .field("peer", &self.peer)
            .field("transport", &self.transport)
            .field("direction", &self.direction)
            .field("state", &self.state)
            .finish()
    }
}

/// A capability that can be presented to the manager for one link write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkDeliveryCapability {
    link_id: LinkId,
    peer: PeerId,
    transport: TransportKind,
}

impl LinkDeliveryCapability {
    /// Returns the selected local link ID.
    pub const fn link_id(self) -> LinkId {
        self.link_id
    }

    /// Returns the selected peer reference.
    pub const fn peer(self) -> PeerId {
        self.peer
    }

    /// Returns the transport kind.
    pub const fn transport(self) -> TransportKind {
        self.transport
    }
}

struct LinkRecord {
    candidate: LinkCandidate,
    created_at: Duration,
    last_termination: Option<TerminationCategory>,
    queued_messages: u64,
    queued_bytes: u64,
    _active_lease: TransportLease,
}

struct ManagerState {
    links: BTreeMap<LinkId, LinkRecord>,
    peers: BTreeMap<PeerId, BTreeSet<LinkId>>,
    dials: BTreeMap<PeerId, DialBackoff>,
    observations: VecDeque<ReachabilityObservation>,
}

/// Shared state held by a manager and queue-accounting guards.
pub(crate) struct ManagerInner {
    resources: TransportResources,
    limits: TransportLimits,
    state: Mutex<ManagerState>,
}

/// RAII guard that decrements one link's queue counters on handoff or drop.
pub(crate) struct QueueAccounting {
    inner: Weak<ManagerInner>,
    link_id: LinkId,
    bytes: u64,
}

impl QueueAccounting {
    pub(crate) fn new(inner: &Arc<ManagerInner>, link_id: LinkId, bytes: u64) -> Self {
        Self {
            inner: Arc::downgrade(inner),
            link_id,
            bytes,
        }
    }
}

impl Drop for QueueAccounting {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        let Ok(mut state) = inner.state.lock() else {
            return;
        };
        let Some(link) = state.links.get_mut(&self.link_id) else {
            return;
        };
        link.queued_messages = link.queued_messages.saturating_sub(1);
        link.queued_bytes = link.queued_bytes.saturating_sub(self.bytes);
    }
}

/// A runtime-neutral manager for bounded transport link ownership.
#[derive(Clone)]
pub struct TransportManager {
    pub(crate) inner: Arc<ManagerInner>,
}

impl fmt::Debug for TransportManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TransportManager(..)")
    }
}

impl TransportManager {
    /// Creates a manager and its shared core resource budget.
    pub fn new(limits: TransportLimits) -> Result<Self, ResourceError> {
        Ok(Self {
            inner: Arc::new(ManagerInner {
                resources: TransportResources::new(limits)?,
                limits,
                state: Mutex::new(ManagerState {
                    links: BTreeMap::new(),
                    peers: BTreeMap::new(),
                    dials: BTreeMap::new(),
                    observations: VecDeque::new(),
                }),
            }),
        })
    }

    /// Returns the immutable ceilings selected for this manager.
    pub fn limits(&self) -> TransportLimits {
        self.inner.limits
    }

    /// Computes the default duplicate policy for an authenticated candidate.
    ///
    /// The returned decision is pure and does not mutate manager state.  A
    /// missing peer slot accepts the candidate; the active link is looked up
    /// only to compare its direction and peer identity.
    pub fn duplicate_resolution(
        &self,
        local_peer: PeerId,
        candidate: &LinkCandidate,
    ) -> Result<DuplicateResolution, RegistrationError> {
        let state = self.lock_state()?;
        let Some(link_ids) = state.peers.get(&candidate.peer()) else {
            return Ok(DuplicateResolution::AcceptNew);
        };
        let Some(existing_id) = link_ids.iter().next() else {
            return Ok(DuplicateResolution::AcceptNew);
        };
        let existing = state
            .links
            .get(existing_id)
            .ok_or(RegistrationError::MissingLink)?;
        Ok(DuplicateLinkPolicy::new(local_peer).decide(&existing.candidate, candidate))
    }

    /// Admits one pending-handshake lease.
    pub fn begin_handshake(
        &self,
        peer: PeerId,
    ) -> Result<PendingHandshake, CandidateAdmissionError> {
        let lease = self
            .inner
            .resources
            .admit(ResourceClass::PendingHandshakes, 1)?;
        Ok(PendingHandshake {
            peer,
            lease: Some(lease),
        })
    }

    /// Alias for begin_handshake.
    pub fn admit_handshake(
        &self,
        peer: PeerId,
    ) -> Result<PendingHandshake, CandidateAdmissionError> {
        self.begin_handshake(peer)
    }

    /// Registers an authenticated candidate with an explicit duplicate policy.
    pub fn register_authenticated(
        &self,
        candidate: LinkCandidate,
        now: Duration,
        duplicate: DuplicateResolution,
    ) -> Result<RegistrationOutcome, RegistrationError> {
        self.register_authenticated_inner(candidate, now, duplicate)
    }

    /// Alias that makes candidate resolution explicit.
    pub fn resolve_candidate(
        &self,
        candidate: LinkCandidate,
        now: Duration,
        duplicate: DuplicateResolution,
    ) -> Result<CandidateDecision, RegistrationError> {
        self.register_authenticated(candidate, now, duplicate)
    }

    fn register_authenticated_inner(
        &self,
        candidate: LinkCandidate,
        now: Duration,
        duplicate: DuplicateResolution,
    ) -> Result<RegistrationOutcome, RegistrationError> {
        if candidate.state() != LinkState::Authenticated {
            return Ok(CandidateDecision::RejectIncompleteAuthentication);
        }
        let link_id = candidate.link_id();
        let peer = candidate.peer();
        let mut state = self.lock_state()?;
        if state.links.contains_key(&link_id) {
            return Err(RegistrationError::DuplicateLinkId);
        }
        let existing = state
            .peers
            .get(&peer)
            .and_then(|links| links.iter().next().copied());

        if let Some(existing) = existing {
            match duplicate {
                DuplicateResolution::RejectNew => {
                    return Ok(CandidateDecision::RejectNewDuplicate { existing });
                }
                DuplicateResolution::RetainExistingDrainNew => {
                    return Ok(CandidateDecision::RetainExistingDrainNew {
                        existing,
                        candidate: link_id,
                    });
                }
                DuplicateResolution::AcceptNew => {
                    let count = state.peers.get(&peer).map_or(0, BTreeSet::len) as u64;
                    if count >= self.inner.limits.max_links_per_peer {
                        return Ok(CandidateDecision::RejectPeerLimit {
                            maximum: self.inner.limits.max_links_per_peer,
                        });
                    }
                    if state.links.len() as u64 >= self.inner.limits.max_active_links {
                        return Ok(CandidateDecision::RejectGlobalLimit {
                            maximum: self.inner.limits.max_active_links,
                        });
                    }
                    let lease = self.admit_active_link()?;
                    self.insert_link(&mut state, candidate, now, lease)?;
                    return Ok(CandidateDecision::AcceptAdditional { link_id });
                }
                DuplicateResolution::ReplaceExisting => {
                    let old = state
                        .links
                        .remove(&existing)
                        .ok_or(RegistrationError::MissingLink)?;
                    remove_peer_link(&mut state, peer, existing);
                    drop(old);
                    let lease = self.admit_active_link()?;
                    self.insert_link(&mut state, candidate, now, lease)?;
                    return Ok(CandidateDecision::ReplaceExisting {
                        existing,
                        candidate: link_id,
                    });
                }
            }
        }

        if state.links.len() as u64 >= self.inner.limits.max_active_links {
            return Ok(CandidateDecision::RejectGlobalLimit {
                maximum: self.inner.limits.max_active_links,
            });
        }
        let lease = self.admit_active_link()?;
        self.insert_link(&mut state, candidate, now, lease)?;
        Ok(CandidateDecision::AcceptFirst { link_id })
    }

    fn admit_active_link(&self) -> Result<TransportLease, RegistrationError> {
        Ok(self.inner.resources.admit(ResourceClass::ActiveLinks, 1)?)
    }

    fn insert_link(
        &self,
        state: &mut ManagerState,
        candidate: LinkCandidate,
        now: Duration,
        active_lease: TransportLease,
    ) -> Result<(), RegistrationError> {
        let id = candidate.link_id();
        let peer = candidate.peer();
        if state
            .links
            .insert(
                id,
                LinkRecord {
                    candidate,
                    created_at: now,
                    last_termination: None,
                    queued_messages: 0,
                    queued_bytes: 0,
                    _active_lease: active_lease,
                },
            )
            .is_some()
        {
            return Err(RegistrationError::DuplicateLinkId);
        }
        state.peers.entry(peer).or_default().insert(id);
        Ok(())
    }

    /// Returns a capability for the first authenticated link for a peer.
    pub fn delivery_capability(
        &self,
        peer: crate::PeerId,
    ) -> Result<LinkDeliveryCapability, DeliveryOutcome> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| DeliveryOutcome::ResourceDenied)?;
        let Some(link_ids) = state.peers.get(&peer) else {
            return Err(DeliveryOutcome::NoActiveLink);
        };
        for link_id in link_ids {
            if let Some(link) = state.links.get(link_id) {
                if link.candidate.state() == LinkState::Authenticated {
                    return Ok(LinkDeliveryCapability {
                        link_id: *link_id,
                        peer,
                        transport: link.candidate.transport(),
                    });
                }
            }
        }
        Err(DeliveryOutcome::LinkClosedBeforeWrite)
    }

    /// Admits a request onto the first authenticated link for its target peer.
    pub fn enqueue_delivery(
        &self,
        request: DeliveryRequest,
        now: Duration,
    ) -> Result<QueuedDelivery, DeliveryOutcome> {
        let capability = self.delivery_capability(request.target())?;
        self.enqueue_on_link(capability, request, now)
    }

    /// Admits a request against an explicit possibly-stale capability.
    pub fn enqueue_on_link(
        &self,
        capability: LinkDeliveryCapability,
        request: DeliveryRequest,
        now: Duration,
    ) -> Result<QueuedDelivery, DeliveryOutcome> {
        if request.target() != capability.peer() {
            return Err(DeliveryOutcome::PeerIdentityMismatch);
        }
        if request.is_cancelled() {
            return Err(DeliveryOutcome::Cancelled);
        }
        if request.deadline().is_elapsed(now) {
            return Err(DeliveryOutcome::DeadlineElapsed);
        }
        if request.is_cancelled() {
            return Err(DeliveryOutcome::Cancelled);
        }
        let length = request.message_len() as u64;
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| DeliveryOutcome::ResourceDenied)?;
        let Some(link) = state.links.get(&capability.link_id()) else {
            return Err(DeliveryOutcome::LinkReplaced);
        };
        if link.candidate.state() != LinkState::Authenticated {
            return Err(DeliveryOutcome::LinkClosedBeforeWrite);
        }
        if link.queued_messages >= self.inner.limits.max_messages_per_link
            || link.queued_bytes.saturating_add(length) > self.inner.limits.max_bytes_per_link
        {
            return Err(DeliveryOutcome::QueueFull);
        }
        drop(state);

        let queue_lease = match self.inner.resources.admit_queue(length) {
            Ok(lease) => lease,
            Err(ResourceError::Exhausted {
                class: ResourceClass::CommandQueueItems,
                ..
            }) => return Err(DeliveryOutcome::QueueFull),
            Err(_) => return Err(DeliveryOutcome::ResourceDenied),
        };

        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| DeliveryOutcome::ResourceDenied)?;
        let Some(link) = state.links.get_mut(&capability.link_id()) else {
            drop(queue_lease);
            return Err(DeliveryOutcome::LinkReplaced);
        };
        if link.candidate.state() != LinkState::Authenticated {
            drop(queue_lease);
            return Err(DeliveryOutcome::LinkClosedBeforeWrite);
        }
        if link.queued_messages >= self.inner.limits.max_messages_per_link
            || link.queued_bytes.saturating_add(length) > self.inner.limits.max_bytes_per_link
        {
            drop(queue_lease);
            return Err(DeliveryOutcome::QueueFull);
        }
        link.queued_messages = match link.queued_messages.checked_add(1) {
            Some(value) => value,
            None => {
                drop(queue_lease);
                return Err(DeliveryOutcome::ResourceDenied);
            }
        };
        link.queued_bytes = match link.queued_bytes.checked_add(length) {
            Some(value) => value,
            None => {
                link.queued_messages = link.queued_messages.saturating_sub(1);
                drop(queue_lease);
                return Err(DeliveryOutcome::ResourceDenied);
            }
        };
        let accounting = QueueAccounting::new(&self.inner, capability.link_id(), length);
        drop(state);
        Ok(QueuedDelivery::new(request, queue_lease, accounting))
    }

    /// Advances one link through its explicit lifecycle.
    pub fn transition_link(
        &self,
        link_id: LinkId,
        next: LinkState,
    ) -> Result<(), RegistrationError> {
        let mut state = self.lock_state()?;
        let link = state
            .links
            .get_mut(&link_id)
            .ok_or(RegistrationError::MissingLink)?;
        link.candidate.state = link.candidate.state().transition(next)?;
        Ok(())
    }

    /// Removes one link owner and records a typed termination category.
    pub fn close_link(
        &self,
        link_id: LinkId,
        reason: TerminationCategory,
    ) -> Result<CloseOutcome, RegistrationError> {
        let mut state = self.lock_state()?;
        let Some(link) = state.links.remove(&link_id) else {
            return Ok(CloseOutcome::Stale { link_id });
        };
        let peer = link.candidate.peer();
        let previous = link.candidate.state();
        remove_peer_link(&mut state, peer, link_id);
        drop(link);
        Ok(CloseOutcome::Closed {
            link_id,
            previous,
            reason,
        })
    }

    /// Records a dial decision without sleeping or retrying.
    pub fn record_dial_backoff(
        &self,
        peer: PeerId,
        backoff: DialBackoff,
    ) -> Result<(), RegistrationError> {
        let mut state = self.lock_state()?;
        if state.dials.len() >= MAX_REACHABILITY_OBSERVATIONS && !state.dials.contains_key(&peer) {
            if let Some(oldest) = state.dials.keys().next().copied() {
                state.dials.remove(&oldest);
            }
        }
        state.dials.insert(peer, backoff);
        Ok(())
    }

    /// Records one bounded address/reachability observation.
    pub fn record_reachability(
        &self,
        observation: ReachabilityObservation,
    ) -> Result<ReachabilityRecordOutcome, RegistrationError> {
        let mut state = self.lock_state()?;
        let evicted = state.observations.len() >= MAX_REACHABILITY_OBSERVATIONS;
        if evicted {
            state.observations.pop_front();
        }
        state.observations.push_back(observation);
        Ok(if evicted {
            ReachabilityRecordOutcome::EvictedOldest
        } else {
            ReachabilityRecordOutcome::Retained
        })
    }

    /// Returns a deterministic, privacy-safe aggregate snapshot.
    pub fn snapshot(&self, now: Duration) -> Result<TransportSnapshot, SnapshotError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| SnapshotError::Resource(ResourceError::Poisoned))?;
        let links = state
            .links
            .values()
            .take(MAX_LINK_SNAPSHOT_ENTRIES)
            .map(|link| {
                let age = now.saturating_sub(link.created_at).min(MAX_DEADLINE);
                LinkSnapshot {
                    link_id: link.candidate.link_id(),
                    transport: link.candidate.transport(),
                    direction: link.candidate.direction(),
                    lifecycle: link.candidate.state(),
                    authenticated: link.candidate.state().is_authenticated(),
                    queued_messages: link.queued_messages,
                    queued_bytes: link.queued_bytes,
                    age,
                    last_termination: link.last_termination,
                    resources: LinkResourceUsage {
                        active_link_units: 1,
                        queued_message_units: link.queued_messages,
                        buffered_byte_units: link.queued_bytes,
                    },
                }
            })
            .collect();
        let observations = state.observations.iter().copied().collect();
        let resources = self
            .inner
            .resources
            .snapshot()
            .map_err(SnapshotError::Resource)?;
        Ok(TransportSnapshot {
            links,
            observations,
            resources,
            dial_backoff_entries: state.dials.len(),
        })
    }

    /// Returns current shared resource usage for one configured class.
    pub fn resource_usage(
        &self,
        class: ResourceClass,
    ) -> Result<i2pr_core::ResourceUsage, ResourceError> {
        self.inner.resources.usage(class)
    }

    /// Borrows the shared transport resource owner for teardown assertions.
    pub fn resources(&self) -> &TransportResources {
        &self.inner.resources
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, ManagerState>, RegistrationError> {
        self.inner
            .state
            .lock()
            .map_err(|_| RegistrationError::StatePoisoned)
    }
}

fn remove_peer_link(state: &mut ManagerState, peer: PeerId, link_id: LinkId) {
    if let Some(links) = state.peers.get_mut(&peer) {
        links.remove(&link_id);
        if links.is_empty() {
            state.peers.remove(&peer);
        }
    }
}

/// An exact pending-handshake lease that releases on every terminal path.
pub struct PendingHandshake {
    peer: PeerId,
    lease: Option<TransportLease>,
}

impl PendingHandshake {
    /// Returns the peer reference associated with this admission.
    pub const fn peer(&self) -> PeerId {
        self.peer
    }

    /// Releases the handshake lease by consuming this owner.
    pub fn release(self) {
        drop(self);
    }

    /// Completes handshake admission and consumes the pending lease.
    pub fn register(
        self,
        manager: &TransportManager,
        candidate: LinkCandidate,
        now: Duration,
        duplicate: DuplicateResolution,
    ) -> Result<RegistrationOutcome, RegistrationError> {
        if candidate.peer() != self.peer {
            return Ok(CandidateDecision::RejectPeerIdentityMismatch);
        }
        manager.register_authenticated(candidate, now, duplicate)
    }
}

impl fmt::Debug for PendingHandshake {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingHandshake")
            .field("peer", &self.peer)
            .field("admitted", &self.lease.is_some())
            .finish()
    }
}
