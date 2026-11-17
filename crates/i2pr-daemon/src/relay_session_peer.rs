//! Plan 534 — the local observation of a peer this router reached through a
//! relay.
//!
//! ## Why the observation exists
//!
//! Plan 533 gave the requester the **establishment**: the requester now holds
//! an authenticated session with the peer it reached through a relay. What
//! the router still could not do was **use** that peer. The product's
//! tunnel-build route draws its peer from the peer-profile store's policy
//! selection, so a peer the router deliberately reached through a relay was
//! one candidate among every peer the router knew, and the deliberate reach
//! was dropped on the floor.
//!
//! This module owns the **observation**: a bounded record that this router
//! reached a peer through a relay. The route reads the observation and builds
//! toward the peer it names.
//!
//! ## What this module owns
//!
//! - a **bounded** table of observations, with a stated capacity and a stated
//!   expiry;
//! - the operator's opt-in: a disabled owner refuses every observation;
//! - and a counter per outcome, so a refusal is never silent.
//!
//! ## What this module never does
//!
//! - It never writes into a peer's profile. The observation is a fact about
//!   **this router's** session with a peer, not a fact about the peer: the
//!   router knows it reached the peer, not that the peer is reachable by
//!   others. No corroborating class is claimed and no reachability signal is
//!   written.
//! - It never advertises anything.
//! - It owns no task, no timer, no listener and no socket. The product's
//!   existing tick advances it.
//!
//! The **mechanics** stay where they belong: the session is the runtime's,
//! the build is the coordinator's, and the peer's material is read from the
//! **validated** store by the driver's existing resolver.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use i2pr_netdb::RouterHash;

/// The capacity of the local observation table.
///
/// The table is bounded: a router that has reached more peers through a relay
/// than it can remember refuses the observation under its own reason rather
/// than growing without bound.
pub const MAX_RELAY_SESSION_PEERS: usize = 8;

/// How long an observation stays usable, in milliseconds.
///
/// The observation is about a **session**. A session that died while the
/// router was not looking must not be built toward, so the observation
/// expires. The expiry is a bound like every other: nothing is retained
/// forever.
pub const RELAY_SESSION_PEER_TTL_MS: u64 = 60_000;

/// One local observation: this router reached this peer through a relay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelaySessionPeer {
    /// The peer this router reached.
    peer: RouterHash,
    /// When the observation was made.
    observed_at_ms: u64,
}

impl RelaySessionPeer {
    /// Creates an observation.
    pub const fn new(peer: RouterHash, observed_at_ms: u64) -> Self {
        Self {
            peer,
            observed_at_ms,
        }
    }

    /// The peer this router reached.
    pub const fn peer(&self) -> RouterHash {
        self.peer
    }

    /// When the observation was made.
    pub const fn observed_at_ms(&self) -> u64 {
        self.observed_at_ms
    }
}

/// Why the router did not record the observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelaySessionPeerRefusal {
    /// The operator left the requester role's opt-in off.
    Disabled,
    /// The observation table is full.
    TableFull,
}

impl RelaySessionPeerRefusal {
    /// A stable, count-only reason key for this refusal.
    ///
    /// The key is a label only: it carries no peer identity, no address and
    /// no key. It exists so a refusal can be counted by reason.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::TableFull => "table-full",
        }
    }
}

/// The owner's counters, one per outcome.
///
/// Every refusal and every outcome is counted under its own reason, so a
/// caller can observe what happened instead of inferring it. The counters are
/// counts only: they name no peer, no address and no key.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelaySessionPeerCounters {
    /// Observations recorded.
    pub observations_recorded: u64,
    /// Observations taken by the route.
    pub observations_taken: u64,
    /// Observations dropped because they expired.
    pub observations_expired: u64,
    /// The operator's opt-in was off.
    pub refusals_disabled: u64,
    /// The observation table was full.
    pub refusals_table_full: u64,
}

/// The bounded owner of this router's local relay observations.
///
/// The owner holds no socket and dials nothing itself: it records that the
/// router reached a peer, and the route takes the observation and builds.
pub struct RelaySessionPeerOwner {
    /// The operator's opt-in. A disabled owner refuses every observation.
    enabled: bool,
    /// The observations, keyed by the peer and stamped with the time.
    peers: BTreeMap<RouterHash, u64>,
    counters: RelaySessionPeerCounters,
}

impl std::fmt::Debug for RelaySessionPeerOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelaySessionPeerOwner")
            .field("enabled", &self.enabled)
            .field("observations", &self.peers.len())
            .finish()
    }
}

/// The shared handle the establishment's task records into and the route
/// takes from.
///
/// The handle is shared because the two live on different sides of the
/// product's tick: the task is an owned async task, and the route is a plain
/// value the tick advances. Every critical section is short and never awaits.
pub type RelaySessionPeerHandle = Arc<Mutex<RelaySessionPeerOwner>>;

impl RelaySessionPeerOwner {
    /// Creates an owner.
    ///
    /// `enabled` is the operator's opt-in. A disabled owner refuses every
    /// observation, so a router that has not asked to reach a peer through a
    /// relay records nothing.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            peers: BTreeMap::new(),
            counters: RelaySessionPeerCounters::default(),
        }
    }

    /// Wraps an owner in the shared handle.
    pub fn into_handle(self) -> RelaySessionPeerHandle {
        Arc::new(Mutex::new(self))
    }

    /// Whether the operator enabled this router's observations.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the owner's counters.
    pub const fn counters(&self) -> RelaySessionPeerCounters {
        self.counters
    }

    /// Records one refusal under its own reason.
    pub fn refuse(&mut self, reason: RelaySessionPeerRefusal) {
        match reason {
            RelaySessionPeerRefusal::Disabled => self.counters.refusals_disabled += 1,
            RelaySessionPeerRefusal::TableFull => self.counters.refusals_table_full += 1,
        }
    }

    /// Records that this router reached `peer` through a relay.
    ///
    /// The observation is a **local** fact: it says nothing about the peer and
    /// writes nothing into the peer's record. Recording the same peer again
    /// refreshes the observation rather than growing the table.
    pub fn record(&mut self, peer: RouterHash, now_ms: u64) -> Result<(), RelaySessionPeerRefusal> {
        if !self.enabled {
            self.refuse(RelaySessionPeerRefusal::Disabled);
            return Err(RelaySessionPeerRefusal::Disabled);
        }
        if !self.peers.contains_key(&peer) && self.peers.len() >= MAX_RELAY_SESSION_PEERS {
            self.refuse(RelaySessionPeerRefusal::TableFull);
            return Err(RelaySessionPeerRefusal::TableFull);
        }
        self.peers.insert(peer, now_ms);
        self.counters.observations_recorded += 1;
        Ok(())
    }

    /// Takes the **oldest** live observation, if there is one.
    ///
    /// Taking is consuming: the observation is removed, so the router builds
    /// toward a peer it reached through a relay **once**. A router that wants
    /// to reach the peer again reaches it again through the relay. Expired
    /// observations are dropped first and counted.
    pub fn take(&mut self, now_ms: u64) -> Option<RouterHash> {
        self.expire(now_ms);
        // The table is bounded and small, so the oldest observation is found
        // by a scan. Ties are broken by the peer's key, so the choice is
        // deterministic.
        let oldest = self
            .peers
            .iter()
            .min_by_key(|(peer, observed_at)| (**observed_at, **peer))
            .map(|(peer, _)| *peer)?;
        self.peers.remove(&oldest);
        self.counters.observations_taken += 1;
        Some(oldest)
    }

    /// The number of live observations.
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Whether the table holds no observation.
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Drops the observations that expired, and counts them.
    fn expire(&mut self, now_ms: u64) {
        let cutoff = now_ms.saturating_sub(RELAY_SESSION_PEER_TTL_MS);
        let expired: Vec<RouterHash> = self
            .peers
            .iter()
            .filter(|(_, observed_at)| **observed_at < cutoff)
            .map(|(peer, _)| *peer)
            .collect();
        for peer in expired {
            self.peers.remove(&peer);
            self.counters.observations_expired += 1;
        }
    }
}

/// Records that this router reached `peer` through a relay.
///
/// The handle's lock is taken for the duration of the call and released
/// before the caller can suspend: the call never awaits.
pub fn record_relay_session_peer(
    handle: &RelaySessionPeerHandle,
    peer: RouterHash,
    now_ms: u64,
) -> Result<(), RelaySessionPeerRefusal> {
    let Ok(mut owner) = handle.lock() else {
        return Err(RelaySessionPeerRefusal::Disabled);
    };
    owner.record(peer, now_ms)
}

/// Takes the oldest live observation from the shared handle.
pub fn take_relay_session_peer(handle: &RelaySessionPeerHandle, now_ms: u64) -> Option<RouterHash> {
    let mut owner = handle.lock().ok()?;
    owner.take(now_ms)
}

/// Reads the owner's counters from the shared handle.
pub fn relay_session_peer_counters(
    handle: &RelaySessionPeerHandle,
) -> Option<RelaySessionPeerCounters> {
    let owner = handle.lock().ok()?;
    Some(owner.counters())
}
