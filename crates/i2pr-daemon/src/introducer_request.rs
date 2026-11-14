//! Plan 524 — the router asks a peer for a relay tag.
//!
//! A router behind a firewall cannot be reached directly, so it needs an
//! **introducer**: a peer that will introduce it to the peers that want to
//! talk to it. The pinned i2pd 2.61.0 asks for a relay tag under exactly
//! one condition
//! (`libi2pd/SSU2Session.cpp:753`):
//!
//! ```cpp
//! if (GetRouterStatus () == eRouterStatusFirewalled && m_Address->IsIntroducer ())
//! ```
//!
//! — the router is **firewalled** and the peer **advertises the introducer
//! capability** (`libi2pd/RouterInfo.h:184`, `caps & eSSUIntroducer`).
//!
//! This module owns the **policy** half of that decision. It:
//!
//! - refuses every request while the operator's opt-in is off;
//! - refuses every request while this router is not firewalled, because a
//!   reachable router needs no introducer;
//! - examines a **bounded**, rotating window of the peers this router holds
//!   a session with, so a busy router sees every peer over time instead of
//!   re-examining the same head of the list for ever;
//! - never names a peer this router has **no session with**: the request
//!   travels in-session, and no new connection is made; and
//! - counts every outcome under its own reason, so a reader observes what
//!   happened instead of inferring it.
//!
//! The owner holds **no** task, **no** timer, **no** listener and **no**
//! socket. The daemon's product tick drives it, exactly as it drives Plan
//! 521's own peer test.
//!
//! The owner does **not** read a peer's capability itself: the read is the
//! daemon's, because the daemon is the tree's authority for peer records
//! and the read needs the authoritative store. The owner takes the answer
//! as data ([`IntroducerCapability`]), which keeps this module free of any
//! store, any lock and any await.
//!
//! The owner does **not** publish anything. A relay tag is public protocol
//! material, but the *publication* of the introducer set this router has
//! learned is a **successor's** subject, and this router still advertises no
//! introducer capability of its own.

use std::collections::BTreeSet;

use i2pr_transport::PeerId;

/// The maximum number of peers one cadence window examines.
///
/// The window is a **bound**, not a target: a router with more peers than
/// this examines the rest over the following windows, so the work one
/// window does never grows with the router's peer population.
pub const MAX_INTRODUCER_CANDIDATES: usize = 16;

/// The cadence between two relay-tag requests, in milliseconds.
///
/// One request leaves the router per window at most, so a router that
/// always finds an eligible peer asks at most once a minute.
pub const INTRODUCER_REQUEST_INTERVAL_MS: u64 = 60_000;

/// The cadence after a window that asked nobody, in milliseconds.
///
/// A window that found no eligible peer is retried sooner, so a router
/// that learns of an introducer does not wait a full interval for it.
pub const INTRODUCER_REQUEST_RETRY_MS: u64 = 15_000;

/// The outcome of one read of a peer's advertised introducer capability.
///
/// The read is the daemon's, from the **validated** peer record the netdb
/// holds. Every refusal is a distinct variant: a peer whose capability
/// cannot be read is **not** an introducer, and never a peer that is
/// merely unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntroducerCapability {
    /// The peer advertises the introducer capability (`C`).
    Advertised,
    /// The peer has no validated record in the authoritative store.
    NoStoredRecord,
    /// The peer's validated record carries no SSU2 address.
    NoSsu2Address,
    /// The peer's SSU2 address did not parse under the transport's strict
    /// parser.
    UnparsableAddress,
    /// The peer's SSU2 address parses but advertises no introducer
    /// capability.
    NotAdvertised,
}

/// Reads one peer's advertised introducer capability from the
/// authoritative store.
///
/// The bytes are the ones the netdb **validated on ingest** — the
/// signature, the identity hash, the static-key binding and the freshness
/// were all checked before the record reached the store — and the address
/// is parsed with the transport's own **strict** parser. A peer with no
/// stored record, with no SSU2 address, with an unparsable address, or with
/// an address that advertises no introducer letter is **not** an
/// introducer, and each is a distinct refusal — never a silent absence.
///
/// The read never mutates the store and never reaches outside it: a peer
/// this router has not learned about is simply not an introducer.
pub fn read_capability(store: &i2pr_netdb::RouterInfoStore, peer: PeerId) -> IntroducerCapability {
    let key = i2pr_netdb::RouterHash::from_hash(peer.hash());
    let Some(record) = store.get(&key) else {
        return IntroducerCapability::NoStoredRecord;
    };
    let Some(address) = record
        .router_info()
        .addresses()
        .iter()
        .find(|address| address.transport_style() == "SSU2")
    else {
        return IntroducerCapability::NoSsu2Address;
    };
    let Ok(parsed) = i2pr_runtime::Ssu2RouterAddress::parse(address) else {
        return IntroducerCapability::UnparsableAddress;
    };
    if parsed.capabilities().supports_relay() {
        IntroducerCapability::Advertised
    } else {
        IntroducerCapability::NotAdvertised
    }
}

impl IntroducerCapability {
    /// Whether the peer is an introducer.
    ///
    /// The test is the reference's own: the peer's **advertised**
    /// capability, never the peer's published introducer set. A peer that
    /// *publishes* introducers is a peer that *has* introducers, not a
    /// peer that is willing to *be* one.
    pub const fn is_introducer(self) -> bool {
        matches!(self, Self::Advertised)
    }
}

/// Why the router did not ask a peer for a relay tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntroducerRequestRefusal {
    /// The operator left the opt-in off.
    Disabled,
    /// This router is not firewalled, so it needs no introducer.
    NotFirewalled,
    /// No peer this router holds a session with advertised the introducer
    /// capability.
    NoEligiblePeer,
    /// The transport refused to queue the request.
    TransportRefused,
}

/// The owner's counters, one per outcome.
///
/// Every refusal and every outcome is counted under its own reason, so a
/// caller can observe what happened instead of inferring it. The counters
/// are counts only: they name no peer, no address, no key and no tag.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IntroducerRequestCounters {
    /// Capability reads that answered affirmatively.
    pub capability_advertised: u64,
    /// Capability reads refused because the peer has no validated record.
    pub capability_refused_no_record: u64,
    /// Capability reads refused because the record carries no SSU2 address.
    pub capability_refused_no_ssu2_address: u64,
    /// Capability reads refused because the SSU2 address did not parse.
    pub capability_refused_unparsable: u64,
    /// Capability reads refused because the address advertises no
    /// introducer capability.
    pub capability_refused_not_advertised: u64,
    /// Relay-tag requests queued toward an eligible peer.
    pub requests_queued: u64,
    /// Requests refused because the operator left the opt-in off.
    pub requests_refused_disabled: u64,
    /// Requests refused because this router is not firewalled.
    pub requests_refused_not_firewalled: u64,
    /// Requests refused because no eligible peer was found.
    pub requests_refused_no_eligible_peer: u64,
    /// Requests refused because the transport refused to queue them.
    pub requests_refused_transport: u64,
}

/// What the owner decided for one window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntroducerRequestDecision {
    /// Ask this peer for a relay tag.
    Ask(PeerId),
    /// Ask nobody; this is why.
    Refuse(IntroducerRequestRefusal),
}

/// The bounded owner of this router's relay-tag requests.
///
/// The owner selects the peer to ask and records every outcome. It sends
/// nothing by itself: the caller queues the request through the runtime,
/// so the request rides the transport's own session and budgets.
#[derive(Debug)]
pub struct IntroducerRequestOwner {
    /// The operator's opt-in. A disabled owner refuses every window.
    enabled: bool,
    /// The peers this router already asked for a tag.
    ///
    /// The set is pruned against the live session peers on every window,
    /// so it never outgrows the router's own peer population.
    asked: BTreeSet<PeerId>,
    /// The rotating cursor: the last peer hash this owner examined.
    ///
    /// The cursor is what makes the window **rotate**: without it a busy
    /// router would re-examine the same head of the peer list for ever
    /// and never look at a peer further down it.
    cursor: Option<[u8; 32]>,
    /// The local time the next window is due.
    next_at_ms: u64,
    counters: IntroducerRequestCounters,
}

impl IntroducerRequestOwner {
    /// Creates an owner.
    ///
    /// `enabled` is the operator's opt-in. A disabled owner refuses every
    /// window, so a router that has not asked to learn an introducer never
    /// sends a relay-tag request.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            asked: BTreeSet::new(),
            cursor: None,
            next_at_ms: 0,
            counters: IntroducerRequestCounters::default(),
        }
    }

    /// Whether the operator enabled this router's relay-tag requests.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the owner's counters.
    pub const fn counters(&self) -> IntroducerRequestCounters {
        self.counters
    }

    /// Whether a window is due at `now_ms`.
    pub const fn due(&self, now_ms: u64) -> bool {
        now_ms >= self.next_at_ms
    }

    /// Returns the bounded window of peers this window may examine.
    ///
    /// `live` is the peers this router currently holds a session with. The
    /// window is drawn from `live` in a stable order and **rotated** past
    /// the peers the previous window examined, so every peer is seen over
    /// time. The `asked` set is pruned against `live` first, so a peer
    /// this router already asked is never asked again while its session
    /// lives.
    pub fn candidate_window(&mut self, live: &[PeerId], limit: usize) -> Vec<PeerId> {
        self.asked.retain(|peer| live.contains(peer));
        let mut ordered: Vec<PeerId> = live.to_vec();
        ordered.sort_by(|left, right| left.hash().as_bytes().cmp(right.hash().as_bytes()));
        let start = match self.cursor {
            Some(cursor) => ordered
                .iter()
                .position(|peer| peer.hash().as_bytes() > &cursor)
                .unwrap_or(0),
            None => 0,
        };
        let window: Vec<PeerId> = ordered
            .iter()
            .skip(start)
            .chain(ordered.iter().take(start))
            .filter(|peer| !self.asked.contains(peer))
            .take(limit)
            .copied()
            .collect();
        if let Some(last) = window.last() {
            self.cursor = Some(*last.hash().as_bytes());
        }
        window
    }

    /// Records one read of a peer's advertised introducer capability.
    pub fn note_capability(&mut self, capability: IntroducerCapability) {
        match capability {
            IntroducerCapability::Advertised => self.counters.capability_advertised += 1,
            IntroducerCapability::NoStoredRecord => {
                self.counters.capability_refused_no_record += 1;
            }
            IntroducerCapability::NoSsu2Address => {
                self.counters.capability_refused_no_ssu2_address += 1;
            }
            IntroducerCapability::UnparsableAddress => {
                self.counters.capability_refused_unparsable += 1;
            }
            IntroducerCapability::NotAdvertised => {
                self.counters.capability_refused_not_advertised += 1;
            }
        }
    }

    /// Decides whether this window asks a peer for a relay tag.
    ///
    /// `eligible` holds the peers of this window that advertised the
    /// introducer capability, in the window's order. `firewalled` is this
    /// router's own reachability state: only a firewalled router asks.
    pub fn decide(
        &mut self,
        eligible: &[PeerId],
        firewalled: bool,
        now_ms: u64,
    ) -> IntroducerRequestDecision {
        if !self.enabled {
            self.counters.requests_refused_disabled += 1;
            self.next_at_ms = now_ms.saturating_add(INTRODUCER_REQUEST_RETRY_MS);
            return IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::Disabled);
        }
        if !firewalled {
            self.counters.requests_refused_not_firewalled += 1;
            self.next_at_ms = now_ms.saturating_add(INTRODUCER_REQUEST_RETRY_MS);
            return IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NotFirewalled);
        }
        let Some(peer) = eligible.first().copied() else {
            self.counters.requests_refused_no_eligible_peer += 1;
            self.next_at_ms = now_ms.saturating_add(INTRODUCER_REQUEST_RETRY_MS);
            return IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NoEligiblePeer);
        };
        self.next_at_ms = now_ms.saturating_add(INTRODUCER_REQUEST_INTERVAL_MS);
        IntroducerRequestDecision::Ask(peer)
    }

    /// Records a request the transport accepted.
    ///
    /// The peer joins the **asked** set here and not before: a request the
    /// transport refused never left the router, so the peer stays eligible.
    pub fn note_queued(&mut self, peer: PeerId) {
        self.counters.requests_queued += 1;
        self.asked.insert(peer);
    }

    /// Records a request the transport refused.
    pub fn note_transport_refusal(&mut self, now_ms: u64) {
        self.counters.requests_refused_transport += 1;
        self.next_at_ms = now_ms.saturating_add(INTRODUCER_REQUEST_RETRY_MS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(byte: u8) -> PeerId {
        PeerId::from_bytes([byte; 32])
    }

    #[test]
    fn a_disabled_owner_refuses_every_window() {
        let mut owner = IntroducerRequestOwner::new(false);
        let decision = owner.decide(&[peer(1)], true, 0);
        assert_eq!(
            decision,
            IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::Disabled)
        );
        assert_eq!(owner.counters().requests_refused_disabled, 1);
        assert_eq!(owner.counters().requests_queued, 0);
    }

    #[test]
    fn a_reachable_router_asks_nobody() {
        let mut owner = IntroducerRequestOwner::new(true);
        let decision = owner.decide(&[peer(1)], false, 0);
        assert_eq!(
            decision,
            IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NotFirewalled)
        );
        assert_eq!(owner.counters().requests_refused_not_firewalled, 1);
    }

    #[test]
    fn a_firewalled_router_asks_the_first_eligible_peer() {
        let mut owner = IntroducerRequestOwner::new(true);
        let decision = owner.decide(&[peer(1), peer(2)], true, 0);
        assert_eq!(decision, IntroducerRequestDecision::Ask(peer(1)));
    }

    #[test]
    fn a_window_without_an_eligible_peer_asks_nobody() {
        let mut owner = IntroducerRequestOwner::new(true);
        let decision = owner.decide(&[], true, 0);
        assert_eq!(
            decision,
            IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NoEligiblePeer)
        );
        assert_eq!(owner.counters().requests_refused_no_eligible_peer, 1);
    }

    #[test]
    fn the_window_rotates_and_the_asked_peer_is_not_asked_again() {
        let mut owner = IntroducerRequestOwner::new(true);
        let live = vec![peer(1), peer(2), peer(3)];
        let first = owner.candidate_window(&live, 1);
        assert_eq!(first, vec![peer(1)]);
        owner.note_queued(peer(1));
        let second = owner.candidate_window(&live, 1);
        assert_eq!(second, vec![peer(2)]);
        let third = owner.candidate_window(&live, 1);
        assert_eq!(third, vec![peer(3)]);
        // The window wrapped: the asked peer is still excluded.
        let fourth = owner.candidate_window(&live, 1);
        assert_eq!(fourth, vec![peer(2)]);
    }

    #[test]
    fn the_asked_set_is_pruned_against_the_live_sessions() {
        let mut owner = IntroducerRequestOwner::new(true);
        let live = vec![peer(1)];
        owner.note_queued(peer(1));
        assert_eq!(owner.candidate_window(&live, 4), Vec::<PeerId>::new());
        // The session is gone: the peer is eligible again.
        assert_eq!(owner.candidate_window(&[], 4), Vec::<PeerId>::new());
    }

    #[test]
    fn every_capability_refusal_is_counted_under_its_own_reason() {
        let mut owner = IntroducerRequestOwner::new(true);
        owner.note_capability(IntroducerCapability::Advertised);
        owner.note_capability(IntroducerCapability::NoStoredRecord);
        owner.note_capability(IntroducerCapability::NoSsu2Address);
        owner.note_capability(IntroducerCapability::UnparsableAddress);
        owner.note_capability(IntroducerCapability::NotAdvertised);
        let counters = owner.counters();
        assert_eq!(counters.capability_advertised, 1);
        assert_eq!(counters.capability_refused_no_record, 1);
        assert_eq!(counters.capability_refused_no_ssu2_address, 1);
        assert_eq!(counters.capability_refused_unparsable, 1);
        assert_eq!(counters.capability_refused_not_advertised, 1);
    }

    #[test]
    fn the_window_is_due_only_after_its_cadence() {
        let mut owner = IntroducerRequestOwner::new(true);
        assert!(owner.due(0));
        owner.decide(&[peer(1)], true, 1_000);
        assert!(!owner.due(1_000));
        assert!(owner.due(1_000 + INTRODUCER_REQUEST_INTERVAL_MS));
    }
}
