//! Plan 522 — the bounded outbound peer-session pool.
//!
//! ## The gap
//!
//! The router dials per need and holds no session between needs. The
//! publication's period is `39 * 60` seconds
//! (`crates/i2pr-daemon/src/router_info_publication.rs:72`) and a session's
//! idle timeout is `300_000` milliseconds
//! (`crates/i2pr-daemon/src/config.rs:1475`), so a session established by one
//! publication is always gone before the next and the publication's
//! ordinary path is the handshake path.
//!
//! ## What this module owns
//!
//! One bounded decision: **which peers the router wants to hold a session
//! to**, and **which of them need a dial right now**.
//!
//! - The population is the router's **own** version-gated floodfill
//!   selection -- the same `select_version_gated_floodfills` the
//!   publication uses
//!   (`crates/i2pr-daemon/src/router_info_publication.rs:544`) -- so the
//!   pool never names a peer Plan 491's version floor would refuse.
//! - The population is bounded by [`MAX_POOLED_PEERS`].
//! - At most [`MAX_DIALS_PER_PERIOD`] dials are planned per period.
//! - A peer that already holds a session is never dialled again.
//! - A peer whose session lapsed is planned again, and the lapse is
//!   counted.
//! - Every refusal is counted by reason. Nothing is skipped silently.
//!
//! ## What this module does not own
//!
//! The owner holds **no** session, **no** socket, **no** task and **no**
//! timer. The transport owns the sessions; the daemon's composition owns
//! the task that awaits the dials, exactly as Plan 519 gave the
//! publication its own task. The owner itself is runtime-neutral and
//! takes its clock from its caller.
//!
//! ## The peer profile
//!
//! The pool touches **no** peer-profile store. A dial outcome is this
//! router's condition as often as the peer's, and the pool cannot tell
//! them apart, so the pool counts its outcomes in its own counters and
//! nowhere else. A local condition must never be written into a peer's
//! profile.

use std::collections::BTreeMap;

use i2pr_netdb::{LookupPolicy, RouterHash, RouterInfoStore, select_version_gated_floodfills};

/// The pool's ceiling: one publication cycle's worth of peers.
///
/// Plan 522: a publication cycle names fifteen floodfills
/// (`PUBLICATION_CYCLE_TARGETS`,
/// `crates/i2pr-daemon/src/router_info_publication.rs:116`), so a pool of
/// fifteen serves **every** batch of a cycle from an established session
/// and the cycle's next round finds the same fifteen still held. A
/// smaller pool would leave the later batches of every cycle on the
/// handshake path, which is the defect this module removes.
///
/// The ceiling is well under the runtime's own active-session ceiling of
/// sixty-four (`crates/i2pr-daemon/src/config.rs:1447`).
pub const MAX_POOLED_PEERS: usize = crate::router_info_publication::PUBLICATION_CYCLE_TARGETS;

/// The pool's cadence, in milliseconds.
///
/// The pool reads its population and plans its dials on this cadence. The
/// period is well under the transport's keepalive interval, so a pooled
/// session that lapses is replaced promptly.
pub const POOL_PERIOD_MS: u64 = 30_000;

/// The pool's per-period dial budget.
///
/// The pool never starts more than this many dials in one period, so a
/// population that is entirely unreachable cannot turn the pool into a
/// handshake storm.
pub const MAX_DIALS_PER_PERIOD: usize = 2;

/// The pool's selection policy.
///
/// Plan 522: the pool reads the **default** policy -- the same one the
/// publication's owner holds
/// (`crates/i2pr-daemon/src/router_info_publication.rs:694`) -- so the two
/// selections agree about the candidate ceiling, the version floor and
/// the deadlines, and there is no second policy to disagree with.
///
/// The version-gated selection returns every **eligible** candidate among
/// the ones it considered, so the pool's own ceiling
/// ([`MAX_POOLED_PEERS`]) is the binding one, and a candidate beyond it
/// is **refused** and counted rather than silently dropped.
fn pool_policy() -> LookupPolicy {
    LookupPolicy::default()
}

/// The outcome of one pool dial.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerPoolDialOutcome {
    /// The session was established.
    Established,
    /// The transport refused the dial.
    Refused,
    /// The dial failed.
    Failed,
    /// The dial was cancelled before it finished.
    Cancelled,
}

/// One peer's state inside the pool.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeerPoolPeerState {
    /// The number of dial attempts made for this peer.
    pub attempts: u32,
    /// Whether a dial is in flight for this peer.
    pub dialing: bool,
    /// Whether the peer held a session when the pool last saw it.
    pub had_session: bool,
    /// Whether the peer was admitted to the pool's population.
    ///
    /// A peer is admitted when the pool holds a session with it, whether
    /// the pool dialled it or found the session already there. The
    /// admission is counted **once** per peer, so a peer whose session
    /// lapses and is re-established is still one admitted peer.
    pub admitted: bool,
}

/// The pool's counters.
///
/// Every refusal has its own reason, so a peer the pool could not reach
/// is distinguishable from a peer the pool never tried.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeerPoolCounters {
    /// The number of times the pool read its population.
    pub population_reads: u64,
    /// The number of candidates the version gate examined.
    pub candidates_examined: u64,
    /// The number of candidates the version gate refused.
    pub candidates_refused: u64,
    /// The number of candidates the pool's ceiling refused.
    pub ceiling_refused: u64,
    /// The number of distinct peers the pool admitted to its population.
    ///
    /// A peer is admitted when the pool holds a session with it, whether
    /// the pool dialled it or found the session already there. A peer is
    /// counted **once**, however many times its session lapses and is
    /// re-established.
    pub peers_admitted: u64,
    /// The number of peers that already held a session.
    pub sessions_reused: u64,
    /// The number of peers whose session lapsed and were planned again.
    pub sessions_lapsed: u64,
    /// The number of peers refused because a dial was already in flight.
    pub dial_in_flight: u64,
    /// The number of peers refused by the pool's per-period dial budget.
    pub budget_refused: u64,
    /// The number of dials the pool started.
    pub dials_started: u64,
    /// The number of dials that established a session.
    pub dials_established: u64,
    /// The number of dials the transport refused.
    pub dials_refused: u64,
    /// The number of dials that failed.
    pub dials_failed: u64,
    /// The number of dials that were cancelled.
    pub dials_cancelled: u64,
    /// The number of peers the selection named that were no longer
    /// in the store when the dial was resolved.
    pub peers_absent: u64,
}

impl PeerPoolCounters {
    /// The counts keyed by their stable reason labels.
    pub fn counts_by_reason(&self) -> BTreeMap<&'static str, u64> {
        let mut counts = BTreeMap::new();
        counts.insert("population-reads", self.population_reads);
        counts.insert("candidates-examined", self.candidates_examined);
        counts.insert("candidates-refused", self.candidates_refused);
        counts.insert("ceiling-refused", self.ceiling_refused);
        counts.insert("peers-admitted", self.peers_admitted);
        counts.insert("sessions-reused", self.sessions_reused);
        counts.insert("sessions-lapsed", self.sessions_lapsed);
        counts.insert("dial-in-flight", self.dial_in_flight);
        counts.insert("budget-refused", self.budget_refused);
        counts.insert("dials-started", self.dials_started);
        counts.insert("dials-established", self.dials_established);
        counts.insert("dials-refused", self.dials_refused);
        counts.insert("dials-failed", self.dials_failed);
        counts.insert("dials-cancelled", self.dials_cancelled);
        counts.insert("peers-absent", self.peers_absent);
        counts
    }
}

/// The bounded outbound peer-session pool.
///
/// The owner is runtime-neutral: it owns no session, no socket, no task
/// and no timer, and it reads its clock from its caller.
pub struct PeerPoolOwner {
    enabled: bool,
    policy: LookupPolicy,
    /// The per-peer state, keyed by the peer's router hash.
    peers: BTreeMap<RouterHash, PeerPoolPeerState>,
    counters: PeerPoolCounters,
}

impl PeerPoolOwner {
    /// Constructs the pool.
    ///
    /// `enabled` is the operator's explicit opt-in. A pool that is not
    /// enabled plans nothing and reads nothing.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            policy: pool_policy(),
            peers: BTreeMap::new(),
            counters: PeerPoolCounters::default(),
        }
    }

    /// Returns whether the pool is enabled.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the pool's counters.
    pub const fn counters(&self) -> &PeerPoolCounters {
        &self.counters
    }

    /// Returns the peers the pool is holding, in router-hash order.
    pub fn peers(&self) -> Vec<RouterHash> {
        self.peers.keys().copied().collect()
    }

    /// Returns one peer's state, if the pool knows it.
    pub fn peer_state(&self, peer: &RouterHash) -> Option<PeerPoolPeerState> {
        self.peers.get(peer).copied()
    }

    /// Reads the population and plans the dials that are due.
    ///
    /// `established` is the set of peers holding at least one live
    /// session, read from the runtime. A peer that holds one is never
    /// dialled again; a peer whose session lapsed is planned again.
    ///
    /// The returned peers are the ones to dial, in the selection's own
    /// order, and never more than [`MAX_DIALS_PER_PERIOD`] of them. A peer
    /// that is planned is marked as dialling, so a second plan in the same
    /// period never starts a second dial for it.
    pub fn plan(
        &mut self,
        store: &RouterInfoStore,
        local_hash: &RouterHash,
        established: &[i2pr_proto::Hash],
    ) -> Vec<RouterHash> {
        if !self.enabled {
            return Vec::new();
        }
        self.counters.population_reads = self.counters.population_reads.saturating_add(1);
        // Plan 545: prefer floodfills, then fall back to any version-eligible
        // peer. A floodfill-only population made the pool empty on a private
        // LAN, where no record can advertise `f` because floodfill eligibility
        // requires a publicly routable address -- so the router kept no SSU2
        // session with the peers it had actually learned.
        let mut selection =
            select_version_gated_floodfills(store, local_hash, &[*local_hash], &self.policy);
        if selection.targets().is_empty() {
            selection = i2pr_netdb::select_version_gated_peers(
                store,
                local_hash,
                &[*local_hash],
                &self.policy,
            );
        }
        self.counters.candidates_examined = self
            .counters
            .candidates_examined
            .saturating_add(u64::from(selection.examined()));
        self.counters.candidates_refused = self
            .counters
            .candidates_refused
            .saturating_add(u64::from(selection.refusals().total()));
        let mut planned = Vec::new();
        for (index, peer) in selection.targets().iter().enumerate() {
            if index >= MAX_POOLED_PEERS {
                // The pool's ceiling **refuses** the candidate; it never
                // evicts a peer it already holds.
                self.counters.ceiling_refused = self.counters.ceiling_refused.saturating_add(1);
                continue;
            }
            let state = self.peers.entry(*peer).or_default();
            let holds_session = established.contains(peer.as_hash());
            if holds_session {
                self.counters.sessions_reused = self.counters.sessions_reused.saturating_add(1);
                if !state.admitted {
                    self.counters.peers_admitted = self.counters.peers_admitted.saturating_add(1);
                    state.admitted = true;
                }
                state.had_session = true;
                state.dialing = false;
                continue;
            }
            if state.had_session {
                // The session the pool was holding is gone.
                self.counters.sessions_lapsed = self.counters.sessions_lapsed.saturating_add(1);
            }
            state.had_session = false;
            if state.dialing {
                self.counters.dial_in_flight = self.counters.dial_in_flight.saturating_add(1);
                continue;
            }
            if planned.len() >= MAX_DIALS_PER_PERIOD {
                self.counters.budget_refused = self.counters.budget_refused.saturating_add(1);
                continue;
            }
            state.dialing = true;
            state.attempts = state.attempts.saturating_add(1);
            self.counters.dials_started = self.counters.dials_started.saturating_add(1);
            planned.push(*peer);
        }
        planned
    }

    /// Records the outcome of one planned dial.
    ///
    /// The peer is released whether the dial succeeded or not, so a failed
    /// dial cannot wedge the pool.
    pub fn record_dial_outcome(&mut self, peer: &RouterHash, outcome: PeerPoolDialOutcome) {
        let Some(state) = self.peers.get_mut(peer) else {
            return;
        };
        state.dialing = false;
        match outcome {
            PeerPoolDialOutcome::Established => {
                // The pool now holds a session with the peer, so the peer is
                // admitted. The admission is counted once, whether the
                // session was established by the pool's own dial or found
                // already there.
                if !state.admitted {
                    self.counters.peers_admitted = self.counters.peers_admitted.saturating_add(1);
                    state.admitted = true;
                }
                state.had_session = true;
                self.counters.dials_established = self.counters.dials_established.saturating_add(1);
            }
            PeerPoolDialOutcome::Refused => {
                self.counters.dials_refused = self.counters.dials_refused.saturating_add(1);
            }
            PeerPoolDialOutcome::Failed => {
                self.counters.dials_failed = self.counters.dials_failed.saturating_add(1);
            }
            PeerPoolDialOutcome::Cancelled => {
                self.counters.dials_cancelled = self.counters.dials_cancelled.saturating_add(1);
            }
        }
    }

    /// Records that a peer the selection named was no longer in the
    /// store.
    ///
    /// The peer is released, so the absence is retried on the next
    /// period rather than wedging the pool.
    pub fn record_peer_absent(&mut self, peer: &RouterHash) {
        self.counters.peers_absent = self.counters.peers_absent.saturating_add(1);
        if let Some(state) = self.peers.get_mut(peer) {
            state.dialing = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_netdb::RouterInfoStoreConfig;
    use i2pr_proto::{Date, Mapping};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    /// The DSM floor, as the version string a peer publishes.
    const FLOOR_VERSION: &str = "0.9.62";

    fn identity(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic identity")
    }

    /// A validated floodfill record that publishes an explicit version.
    ///
    /// `version` is `None` for a record that publishes no version at all,
    /// which the version gate refuses under its own reason.
    fn floodfill(seed: u64, version: Option<&str>) -> i2pr_netdb::ValidatedRouterInfo {
        let bundle = identity(seed);
        let mut options = Mapping::builder();
        options
            .insert("caps".to_owned(), "f".to_owned())
            .expect("caps");
        if let Some(version) = version {
            options
                .insert("router.version".to_owned(), version.to_owned())
                .expect("version");
        }
        let published = Date::from_millis(1_700_000_000_000);
        let info = bundle
            .sign_router_info(
                published,
                Vec::new(),
                Vec::new(),
                options.build().expect("options"),
            )
            .expect("sign");
        i2pr_netdb::ValidatedRouterInfo::from_router_info(
            info,
            None,
            i2pr_netdb::ValidationContext::new(published),
        )
        .expect("validate")
    }

    /// A store holding `count` eligible floodfills.
    fn store_with(count: usize) -> RouterInfoStore {
        let mut store =
            RouterInfoStore::with_config(RouterInfoStoreConfig::new(256, 16 * 1024 * 1024));
        for index in 0..count {
            store.insert(floodfill(9_000 + index as u64, Some(FLOOR_VERSION)));
        }
        store
    }

    /// The router's own hash, excluded from the population.
    fn local_hash() -> RouterHash {
        RouterHash::from_bytes([0x11; 32])
    }

    #[test]
    fn the_pool_is_off_by_default_and_plans_nothing() {
        let store = store_with(4);
        let mut owner = PeerPoolOwner::new(false);
        assert!(!owner.enabled());
        assert!(owner.plan(&store, &local_hash(), &[]).is_empty());
        assert_eq!(owner.counters().population_reads, 0);
    }

    #[test]
    fn the_ceiling_is_one_publication_cycle() {
        assert_eq!(
            MAX_POOLED_PEERS,
            crate::router_info_publication::PUBLICATION_CYCLE_TARGETS
        );
    }

    #[test]
    fn the_population_is_the_version_gated_floodfill_selection() {
        let store = store_with(4);
        let mut owner = PeerPoolOwner::new(true);
        let planned = owner.plan(&store, &local_hash(), &[]);
        assert_eq!(
            planned.len(),
            MAX_DIALS_PER_PERIOD,
            "the pool plans at most its dial budget"
        );
        assert_eq!(owner.counters().peers_admitted, 0);
        assert_eq!(owner.counters().dials_started, 2);
        assert_eq!(owner.counters().candidates_examined, 4);
        assert_eq!(owner.counters().candidates_refused, 0);
    }

    #[test]
    fn a_peer_below_the_version_floor_is_never_admitted() {
        let mut store =
            RouterInfoStore::with_config(RouterInfoStoreConfig::new(256, 16 * 1024 * 1024));
        store.insert(floodfill(9_001, Some("0.9.61")));
        store.insert(floodfill(9_002, None));
        store.insert(floodfill(9_003, Some(FLOOR_VERSION)));
        let mut owner = PeerPoolOwner::new(true);
        owner.plan(&store, &local_hash(), &[]);
        assert_eq!(owner.counters().candidates_examined, 3);
        assert_eq!(
            owner.counters().candidates_refused,
            2,
            "both the below-floor and the version-less peer are refused"
        );
        assert_eq!(owner.counters().dials_started, 1);
    }

    #[test]
    fn the_pool_refuses_a_candidate_beyond_its_ceiling() {
        let store = store_with(MAX_POOLED_PEERS + 5);
        let mut owner = PeerPoolOwner::new(true);
        owner.plan(&store, &local_hash(), &[]);
        assert_eq!(
            owner.counters().ceiling_refused,
            5,
            "the ceiling refuses the surplus rather than evicting a peer"
        );
        assert_eq!(owner.peers().len(), MAX_POOLED_PEERS);
    }

    #[test]
    fn a_peer_that_holds_a_session_is_never_dialled_again() {
        let store = store_with(4);
        let mut owner = PeerPoolOwner::new(true);
        // The first plan names the peers; the established set then covers
        // all of them, so the second plan dials nothing.
        let planned = owner.plan(&store, &local_hash(), &[]);
        assert!(!planned.is_empty());
        let established: Vec<i2pr_proto::Hash> =
            owner.peers().iter().map(|peer| *peer.as_hash()).collect();
        owner.record_dial_outcome(&planned[0], PeerPoolDialOutcome::Established);
        let second = owner.plan(&store, &local_hash(), &established);
        assert!(second.is_empty(), "a peer with a session is not dialled");
        assert_eq!(
            owner.counters().sessions_reused,
            u64::try_from(store.len()).expect("count fits"),
            "every peer in the store holds a session"
        );
        assert_eq!(owner.counters().dials_started, 2);
    }

    #[test]
    fn a_peer_the_pool_dialled_is_admitted_when_the_dial_establishes() {
        let store = store_with(4);
        let mut owner = PeerPoolOwner::new(true);
        let planned = owner.plan(&store, &local_hash(), &[]);
        assert_eq!(
            owner.counters().peers_admitted,
            0,
            "a planned dial is not an admission"
        );
        owner.record_dial_outcome(&planned[0], PeerPoolDialOutcome::Established);
        assert_eq!(
            owner.counters().peers_admitted,
            1,
            "the pool's own established dial admits the peer"
        );
        // The peer is seen again with its session held, and the admission is
        // not counted twice.
        let established = vec![*planned[0].as_hash()];
        owner.plan(&store, &local_hash(), &established);
        assert_eq!(
            owner.counters().peers_admitted,
            1,
            "an admission is counted once per peer"
        );
    }

    #[test]
    fn a_peer_whose_session_lapsed_is_planned_again() {
        let store = store_with(2);
        let mut owner = PeerPoolOwner::new(true);
        let planned = owner.plan(&store, &local_hash(), &[]);
        assert_eq!(planned.len(), 2);
        for peer in &planned {
            owner.record_dial_outcome(peer, PeerPoolDialOutcome::Established);
        }
        // The sessions are gone: the pool notices the lapse and plans the
        // peers again.
        let again = owner.plan(&store, &local_hash(), &[]);
        assert_eq!(again.len(), 2);
        assert_eq!(owner.counters().sessions_lapsed, 2);
    }

    #[test]
    fn the_pool_never_exceeds_its_dial_budget() {
        let store = store_with(MAX_POOLED_PEERS);
        let mut owner = PeerPoolOwner::new(true);
        let planned = owner.plan(&store, &local_hash(), &[]);
        assert_eq!(planned.len(), MAX_DIALS_PER_PERIOD);
        assert_eq!(
            owner.counters().budget_refused,
            u64::try_from(MAX_POOLED_PEERS - MAX_DIALS_PER_PERIOD).expect("count fits")
        );
    }

    #[test]
    fn a_peer_is_dialled_once_at_a_time() {
        let store = store_with(4);
        let mut owner = PeerPoolOwner::new(true);
        let planned = owner.plan(&store, &local_hash(), &[]);
        assert_eq!(planned.len(), 2);
        // The peers are still dialling, so a second plan in the same
        // period starts no second dial for them.
        let second = owner.plan(&store, &local_hash(), &[]);
        assert!(
            second.iter().all(|peer| !planned.contains(peer)),
            "a peer that is dialling is not dialled again"
        );
        assert_eq!(owner.counters().dial_in_flight, 2);
        assert_eq!(owner.counters().dials_started, 4);
    }

    #[test]
    fn a_failed_dial_is_counted_under_its_own_reason_and_releases_the_peer() {
        let store = store_with(4);
        let mut owner = PeerPoolOwner::new(true);
        let planned = owner.plan(&store, &local_hash(), &[]);
        owner.record_dial_outcome(&planned[0], PeerPoolDialOutcome::Failed);
        owner.record_dial_outcome(&planned[1], PeerPoolDialOutcome::Refused);
        assert_eq!(owner.counters().dials_failed, 1);
        assert_eq!(owner.counters().dials_refused, 1);
        // The failed peer is released, so it is retried on the next
        // period rather than wedging the pool.
        let state = owner.peer_state(&planned[0]).expect("peer state");
        assert!(!state.dialing);
        assert_eq!(state.attempts, 1);
    }

    #[test]
    fn an_absent_peer_is_refused_under_its_own_reason() {
        let store = store_with(2);
        let mut owner = PeerPoolOwner::new(true);
        let planned = owner.plan(&store, &local_hash(), &[]);
        owner.record_peer_absent(&planned[0]);
        assert_eq!(owner.counters().peers_absent, 1);
        let state = owner.peer_state(&planned[0]).expect("peer state");
        assert!(!state.dialing);
    }

    #[test]
    fn the_counters_carry_a_stable_reason_for_every_outcome() {
        let counters = PeerPoolCounters::default();
        let reasons = counters.counts_by_reason();
        for expected in [
            "population-reads",
            "candidates-examined",
            "candidates-refused",
            "ceiling-refused",
            "peers-admitted",
            "sessions-reused",
            "sessions-lapsed",
            "dial-in-flight",
            "budget-refused",
            "dials-started",
            "dials-established",
            "dials-refused",
            "dials-failed",
            "dials-cancelled",
            "peers-absent",
        ] {
            assert!(reasons.contains_key(expected), "{expected} is a reason");
        }
    }
}
