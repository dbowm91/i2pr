//! Plan 506 — the peer-knowledge reconcile.
//!
//! ## The gap
//!
//! The router accepts a peer's RouterInfo on a **live** session: three
//! production sites ingest an authenticated unsolicited `DatabaseStore` into
//! the bootstrap NetDB (`crates/i2pr-daemon/src/lib.rs`). The two stores the
//! router actually **builds with** — the destination coordinator's
//! authoritative store and the peer-profile store — were seeded **once at
//! startup** and never again, so a peer the router learned after startup was
//! invisible to both. The population Plan 505's exploratory driver draws from
//! therefore never grew.
//!
//! The router does **not** learn peers from a NetDB lookup: a
//! `DatabaseSearchReply` carries peer hashes only, and the production router
//! runs no RouterInfo-kind lookup. The live-learned RouterInfos are exactly
//! the unsolicited `DatabaseStore`s, and this module is the path that carries
//! them into the stores the router builds with.
//!
//! ## What this module owns
//!
//! One bounded reconcile:
//!
//! 1. **snapshot** the bootstrap's validated RouterInfos under the bootstrap's
//!    lock, and **release** that lock;
//! 2. **insert** each record into the destination store through the canonical
//!    `insert_validated_router_info`;
//! 3. **learn** the same records into the peer-profile store through the
//!    canonical `learn_all`.
//!
//! The three locks are taken in **one order** and are **never nested**, so no
//! cycle is possible: the bootstrap's lock is released before the
//! destination coordinator's lock is taken, and the destination coordinator's
//! lock is released before the peer-profile store's lock is taken.
//!
//! Every outcome is counted by reason. A capacity refusal, a conflict and a
//! stale replacement are **counted, never fatal** — one bad record does not
//! abort the batch.
//!
//! The reconcile writes **peer records**, never a selection. Plan 491's
//! version gate stays at selection, so a refreshed peer below the floor is
//! still refused there, and counted.
//!
//! No task, no timer, no listener and no socket is added: the reconcile is a
//! step on the daemon product's **existing** tick, gated by a bounded cadence.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex as StdMutex};

use i2pr_netdb::{InsertOutcome, ValidatedRouterInfo};
use tokio::sync::Mutex;

use crate::bootstrap::Bootstrap;
use crate::destination_tunnels::DestinationTunnelCoordinator;
use crate::netdb_participation::{LearnSummary, PeerParticipationHandle};

/// The minimum reconcile cadence, in milliseconds.
///
/// Plan 506: the reconcile reuses the NetDB participation maintenance
/// cadence, whose own floor is one second. The floor is repeated here so a
/// reconcile constructed outside the configuration parser still refuses an
/// out-of-range cadence.
pub const MIN_RECONCILE_PERIOD_MS: u64 = 1_000;

/// The maximum reconcile cadence, in milliseconds.
///
/// The ceiling is the participation driver's own one-hour ceiling, so the
/// reconcile cannot outlive the cadence it shares.
pub const MAX_RECONCILE_PERIOD_MS: u64 = 3_600_000;

/// The router's own RouterInfo **acceptance** window, in milliseconds.
///
/// Plan 507: the retention window is the acceptance window, read from the
/// validation policy rather than restated, so the router retains exactly
/// what it accepts. A second value would be a second source of truth and
/// could disagree with the validator -- which is the defect this plan
/// removes.
///
/// The window is derived through `as_secs`, which is a `const fn`, so the
/// constant cannot drift from the policy without a compile error.
pub const RETENTION_WINDOW_MS: u64 = acceptance_window_secs() * 1_000;

/// The acceptance window in whole seconds.
///
/// The window is a whole number of seconds in the policy, so the
/// millisecond conversion is exact.
const fn acceptance_window_secs() -> u64 {
    i2pr_netdb::ValidationPolicy::default_const()
        .max_age
        .as_secs()
}

/// Why a reconcile could not be constructed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PeerReconcileError {
    /// The cadence was outside the accepted bound.
    #[error("the reconcile cadence {period_ms} ms is outside the accepted bound")]
    InvalidPeriod {
        /// The refused cadence.
        period_ms: u64,
    },
}

/// Bounded, privacy-safe counters for the peer-knowledge reconcile.
///
/// The counters carry counts only: no peer identity, no address, no key
/// material and no payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeerReconcileCounters {
    /// Reconciles that ran.
    pub runs: u64,
    /// Ticks the cadence deferred.
    pub cadence_skips: u64,
    /// Reconciles that could not read the bootstrap store.
    pub source_unavailable: u64,
    /// Reconciles that could not lock the peer-profile store.
    pub store_unavailable: u64,
    /// Peer records the snapshot carried.
    pub snapshot_records: u64,
    /// Records the destination store accepted as new.
    pub inserted: u64,
    /// Records the destination store accepted as a newer replacement.
    pub replaced: u64,
    /// Records the destination store recognised as byte-identical.
    pub idempotent: u64,
    /// Records the destination store refused as a conflict.
    pub conflict: u64,
    /// Records the destination store refused as stale.
    pub stale: u64,
    /// Records the destination store refused because it was at capacity.
    pub capacity_exceeded: u64,
    /// Records the peer-profile store accepted as new.
    pub learned_inserted: u64,
    /// Records the peer-profile store accepted as a newer replacement.
    pub learned_replaced: u64,
    /// Records the peer-profile store recognised as byte-identical.
    pub learned_idempotent: u64,
    /// Records the peer-profile store refused as a conflict.
    pub learned_conflict: u64,
    /// Records the peer-profile store refused as stale.
    pub learned_stale: u64,
    /// Records the peer-profile store refused because its store was at
    /// capacity.
    pub learned_capacity_exceeded: u64,
    /// Records the peer-profile store refused because the profile tier was at
    /// capacity.
    pub learned_profile_capacity_exceeded: u64,
    /// Plan 507 — records the bootstrap's NetDB dropped by a retention
    /// prune.
    pub pruned_bootstrap: u64,
    /// Plan 507 — records the destination coordinator's store dropped by a
    /// retention prune.
    pub pruned_destination: u64,
    /// Plan 507 — records a prune that could not run, so the store
    /// kept its contents and the condition stayed visible.
    pub prune_unavailable: u64,
}

impl PeerReconcileCounters {
    /// Records one destination-store outcome by reason.
    fn record_insert(&mut self, outcome: InsertOutcome) {
        match outcome {
            InsertOutcome::Inserted => self.inserted = self.inserted.saturating_add(1),
            InsertOutcome::Replaced => self.replaced = self.replaced.saturating_add(1),
            InsertOutcome::Idempotent => self.idempotent = self.idempotent.saturating_add(1),
            InsertOutcome::Conflict => self.conflict = self.conflict.saturating_add(1),
            InsertOutcome::StaleReplacement => self.stale = self.stale.saturating_add(1),
            InsertOutcome::CapacityExceeded => {
                self.capacity_exceeded = self.capacity_exceeded.saturating_add(1);
            }
        }
    }

    /// Records one peer-profile-store batch outcome by reason.
    fn record_learn(&mut self, summary: LearnSummary) {
        self.learned_inserted = self
            .learned_inserted
            .saturating_add(summary.inserted as u64);
        self.learned_replaced = self
            .learned_replaced
            .saturating_add(summary.replaced as u64);
        self.learned_idempotent = self
            .learned_idempotent
            .saturating_add(summary.idempotent as u64);
        self.learned_conflict = self
            .learned_conflict
            .saturating_add(summary.conflict as u64);
        self.learned_stale = self.learned_stale.saturating_add(summary.stale as u64);
        self.learned_capacity_exceeded = self
            .learned_capacity_exceeded
            .saturating_add(summary.capacity_exceeded as u64);
        self.learned_profile_capacity_exceeded = self
            .learned_profile_capacity_exceeded
            .saturating_add(summary.profile_capacity_exceeded as u64);
    }

    /// The counts keyed by their stable reason labels.
    ///
    /// The labels are stable strings so a reader can diff two snapshots
    /// without a schema. No peer, address, key or payload appears here.
    pub fn counts_by_reason(&self) -> BTreeMap<&'static str, u64> {
        let mut counts = BTreeMap::new();
        counts.insert("runs", self.runs);
        counts.insert("cadence-skips", self.cadence_skips);
        counts.insert("source-unavailable", self.source_unavailable);
        counts.insert("store-unavailable", self.store_unavailable);
        counts.insert("snapshot-records", self.snapshot_records);
        counts.insert("inserted", self.inserted);
        counts.insert("replaced", self.replaced);
        counts.insert("idempotent", self.idempotent);
        counts.insert("conflict", self.conflict);
        counts.insert("stale", self.stale);
        counts.insert("capacity-exceeded", self.capacity_exceeded);
        counts.insert("learned-inserted", self.learned_inserted);
        counts.insert("learned-replaced", self.learned_replaced);
        counts.insert("learned-idempotent", self.learned_idempotent);
        counts.insert("learned-conflict", self.learned_conflict);
        counts.insert("learned-stale", self.learned_stale);
        counts.insert("learned-capacity-exceeded", self.learned_capacity_exceeded);
        counts.insert(
            "learned-profile-capacity-exceeded",
            self.learned_profile_capacity_exceeded,
        );
        counts.insert("pruned-bootstrap", self.pruned_bootstrap);
        counts.insert("pruned-destination", self.pruned_destination);
        counts.insert("prune-unavailable", self.prune_unavailable);
        counts
    }
}

/// What one reconcile did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerReconcileAction {
    /// The cadence deferred the reconcile.
    Deferred,
    /// The bootstrap store could not be read.
    SourceUnavailable,
    /// The reconcile ran and carried this many records.
    Reconciled {
        /// The number of peer records the snapshot carried.
        records: usize,
    },
}

/// Plan 507 — the outcome of one retention prune.
///
/// The prune either ran, had nothing to do, or could not run. A prune
/// that could not run is **counted**, never reported as a prune that
/// happened: the store keeps its contents and the condition is visible
/// under a stable reason label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PruneOutcome {
    /// The prune ran and removed the given number of records.
    Removed(usize),
    /// The store does not exist in this composition, so there was nothing
    /// to prune and nothing was skipped.
    NotApplicable,
    /// The store's lock could not be taken, so the prune did not run and
    /// the store keeps its contents.
    Unavailable,
}

/// The Plan 506 peer-knowledge reconcile.
///
/// The reconcile owns the three handles it needs and the bounded cadence it
/// runs on. It holds no growing collection: the only state it keeps is the
/// last run's timestamp and its counters.
pub struct PeerReconcile {
    /// The daemon's bootstrap store, when the composition has one.
    bootstrap: Option<Arc<StdMutex<Bootstrap>>>,
    /// The destination coordinator's authoritative store.
    destinations: Arc<Mutex<DestinationTunnelCoordinator>>,
    /// The daemon's peer-profile store, when the composition has one.
    profiles: Option<PeerParticipationHandle>,
    /// The cadence, in milliseconds.
    period_ms: u64,
    /// The last run's timestamp, used for the cadence gate.
    last_run_ms: Option<u64>,
    /// The reconcile's bounded counters.
    counters: PeerReconcileCounters,
}

impl PeerReconcile {
    /// Constructs a reconcile over the three handles it needs.
    ///
    /// The cadence is bounded on both sides. A zero, sub-second or
    /// above-one-hour cadence is refused: the reconcile never runs more often
    /// than the participation maintenance it shares its cadence with, and
    /// never less often than a second.
    pub fn new(
        bootstrap: Option<Arc<StdMutex<Bootstrap>>>,
        destinations: Arc<Mutex<DestinationTunnelCoordinator>>,
        profiles: Option<PeerParticipationHandle>,
        period_ms: u64,
    ) -> Result<Self, PeerReconcileError> {
        if !(MIN_RECONCILE_PERIOD_MS..=MAX_RECONCILE_PERIOD_MS).contains(&period_ms) {
            return Err(PeerReconcileError::InvalidPeriod { period_ms });
        }
        Ok(Self {
            bootstrap,
            destinations,
            profiles,
            period_ms,
            last_run_ms: None,
            counters: PeerReconcileCounters::default(),
        })
    }

    /// Returns the reconcile's counters.
    pub fn counters(&self) -> PeerReconcileCounters {
        self.counters
    }

    /// Returns the cadence the reconcile runs on.
    pub const fn period_ms(&self) -> u64 {
        self.period_ms
    }

    /// Runs one peer-knowledge maintenance pass when its cadence is due.
    ///
    /// The method is a **step**, not a task: it holds no timer and owns no
    /// cancellation. The caller drives it from the product's existing tick.
    ///
    /// Plan 507: the pass prunes first. The router's peer records must not
    /// outlive the router's own acceptance window, and a store that reaches
    /// its quota must make room rather than refuse every new peer forever.
    /// The prune is the tree's own explicit-prune operation (Plan 103
    /// section 3.4), with the acceptance window as the retention window.
    ///
    /// The order is prune the bootstrap, prune the destination store,
    /// snapshot, insert, advertiser insert, learn. The locks are taken one at
    /// a time and never nested: the bootstrap's lock is released before
    /// the destination coordinator's lock is taken, and the destination
    /// coordinator's lock is released before the peer-profile store's lock.
    pub async fn run(&mut self, now_ms: u64) -> PeerReconcileAction {
        if !self.cadence_due(now_ms) {
            self.counters.cadence_skips = self.counters.cadence_skips.saturating_add(1);
            return PeerReconcileAction::Deferred;
        }
        self.last_run_ms = Some(now_ms);
        self.counters.runs = self.counters.runs.saturating_add(1);
        // Plan 507 - the retention prune runs first, on the same cadence
        // as the reconcile, so the two can never drift apart in time. The
        // prune makes room for the writes below, and the snapshot is the
        // fresh set: a record the prune removed is never carried into the
        // destination store.
        self.prune_stores(now_ms);
        // Step 1 — the bootstrap's lock is taken for the snapshot and
        // released before any other lock is taken.
        let Some(records) = self.snapshot() else {
            self.counters.source_unavailable = self.counters.source_unavailable.saturating_add(1);
            return PeerReconcileAction::SourceUnavailable;
        };
        self.counters.snapshot_records = self
            .counters
            .snapshot_records
            .saturating_add(records.len() as u64);
        // Step 2 — the destination coordinator's lock is taken for the
        // inserts and released before the peer-profile store's lock is taken.
        self.insert_all(&records).await;
        // Plan 541 — the destination coordinator's store is the **authority**
        // the router's floodfill selection reads, so the advertisers the
        // NetDB holds must reach it too. The advertiser snapshot is taken
        // under the bootstrap's lock and released before the coordinator's
        // lock is taken, so the two locks are never held at once.
        if let Some(advertisers) = self.advertiser_snapshot() {
            self.insert_advertisers(&advertisers).await;
        }
        // Step 3 — the peer-profile store's lock is taken for the learn. The
        // closure never awaits, so no lock is held across an await.
        self.learn_all(&records);
        PeerReconcileAction::Reconciled {
            records: records.len(),
        }
    }

    /// Plan 507 — prunes the two live stores with the retention window.
    ///
    /// The bootstrap's lock is taken and released here, and the destination
    /// coordinator's lock is taken and released after it. The two are never
    /// held at once, and the peer-profile store's lock is not touched at
    /// all: the profile store has its own eviction policy and self-corrects
    /// through the observation tiers, so it needs no age prune.
    ///
    /// The method takes no await at all, so no lock is ever held across
    /// one.
    ///
    /// A prune that cannot run is counted, never silently skipped: the
    /// store keeps its contents and the condition is visible in the
    /// counters under a stable reason label.
    fn prune_stores(&mut self, now_ms: u64) {
        match self.prune_bootstrap(now_ms) {
            PruneOutcome::Removed(removed) => {
                self.counters.pruned_bootstrap = self
                    .counters
                    .pruned_bootstrap
                    .saturating_add(removed as u64);
            }
            PruneOutcome::Unavailable => {
                self.counters.prune_unavailable = self.counters.prune_unavailable.saturating_add(1);
            }
            PruneOutcome::NotApplicable => {}
        }
        match self.prune_destinations(now_ms) {
            PruneOutcome::Removed(removed) => {
                self.counters.pruned_destination = self
                    .counters
                    .pruned_destination
                    .saturating_add(removed as u64);
            }
            PruneOutcome::Unavailable => {
                self.counters.prune_unavailable = self.counters.prune_unavailable.saturating_add(1);
            }
            PruneOutcome::NotApplicable => {}
        }
    }

    /// Plan 507 — prunes the bootstrap's NetDB under its lock.
    ///
    /// The lock is released when this function returns, before the
    /// destination coordinator's lock is taken.
    ///
    /// A poisoned lock is **not** skipped silently: the prune did not run,
    /// the store keeps its contents, and the caller counts the condition.
    fn prune_bootstrap(&self, now_ms: u64) -> PruneOutcome {
        let Some(bootstrap) = self.bootstrap.as_ref() else {
            return PruneOutcome::NotApplicable;
        };
        let Ok(mut guard) = bootstrap.lock() else {
            return PruneOutcome::Unavailable;
        };
        PruneOutcome::Removed(guard.prune(now_ms, RETENTION_WINDOW_MS))
    }

    /// Plan 507 — prunes the destination coordinator's authoritative
    /// store under its lock.
    ///
    /// The lock is released when this function returns.
    ///
    /// The lock is **tried**, never waited for: a store another task is
    /// using is left alone for this pass, the prune is counted as
    /// unavailable, and the reconcile still runs. The store turns over on
    /// the next pass, and the router never reports a prune it did not
    /// perform.
    fn prune_destinations(&self, now_ms: u64) -> PruneOutcome {
        let destinations = Arc::clone(&self.destinations);
        let Ok(mut guard) = destinations.try_lock() else {
            return PruneOutcome::Unavailable;
        };
        PruneOutcome::Removed(guard.prune(now_ms, RETENTION_WINDOW_MS))
    }

    /// Returns whether the cadence is due.
    fn cadence_due(&self, now_ms: u64) -> bool {
        match self.last_run_ms {
            None => true,
            Some(last) => now_ms.saturating_sub(last) >= self.period_ms,
        }
    }

    /// Takes the bootstrap's lock, snapshots the validated RouterInfos and
    /// **releases** the lock.
    ///
    /// The snapshot is the bounded one the bootstrap already publishes, so a
    /// large NetDB cannot turn into an unbounded copy. The lock guard is
    /// dropped when this function returns, before any other lock is taken.
    fn snapshot(&self) -> Option<Vec<ValidatedRouterInfo>> {
        let bootstrap = self.bootstrap.as_ref()?;
        let guard = bootstrap.lock().ok()?;
        Some(guard.validated_router_info_snapshot())
    }

    /// Takes the bootstrap's lock, snapshots the **floodfill advertisers** and
    /// **releases** the lock.
    ///
    /// Plan 541: the destination coordinator's store is the **authority** the
    /// router's floodfill selection reads, so that store must carry the
    /// advertisers the NetDB holds -- not the group provider's hash-ordered
    /// prefix, which is an arbitrary subset of the population. The snapshot
    /// is taken under the bootstrap's lock and released before this function
    /// returns, so no other lock is ever held with it.
    fn advertiser_snapshot(&self) -> Option<Vec<ValidatedRouterInfo>> {
        let bootstrap = self.bootstrap.as_ref()?;
        let guard = bootstrap.lock().ok()?;
        Some(guard.floodfill_advertiser_snapshot())
    }

    /// Inserts every record into the destination store through the canonical
    /// write API.
    ///
    /// The store's own capacity policy applies: a refusal is counted and the
    /// batch continues. One peer at capacity never aborts the reconcile.
    async fn insert_all(&mut self, records: &[ValidatedRouterInfo]) {
        let destinations = Arc::clone(&self.destinations);
        let mut guard = destinations.lock().await;
        for record in records {
            let outcome = guard.insert_validated_router_info(record.clone());
            self.counters.record_insert(outcome);
        }
    }

    /// Inserts every floodfill advertiser into the destination store through
    /// the canonical write API.
    ///
    /// Plan 541: the coordinator's store is the **authority** the router's
    /// floodfill selection reads, so the advertisers the NetDB holds must
    /// reach it. The store's own capacity policy applies: a refusal is
    /// counted and the batch continues, so one peer at capacity never aborts
    /// the reconcile.
    async fn insert_advertisers(&mut self, records: &[ValidatedRouterInfo]) {
        let destinations = Arc::clone(&self.destinations);
        let mut guard = destinations.lock().await;
        for record in records {
            let outcome = guard.insert_validated_router_info(record.clone());
            self.counters.record_insert(outcome);
        }
    }

    /// Learns every record into the peer-profile store through the canonical
    /// write API.
    ///
    /// The batch is learned with **one** receipt, so a large NetDB cannot
    /// churn the bounded receipt log.
    fn learn_all(&mut self, records: &[ValidatedRouterInfo]) {
        let Some(profiles) = self.profiles.clone() else {
            return;
        };
        let summary = profiles.with(|driver| driver.learn_all(records.iter().cloned()));
        match summary {
            Some(summary) => self.counters.record_learn(summary),
            None => {
                self.counters.store_unavailable = self.counters.store_unavailable.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc as StdArc;

    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_netdb::{ProfileStoreConfig, RouterInfoStoreConfig, ValidationContext};
    use i2pr_proto::{DatabaseStoreData, DatabaseStoreMessage, Date, Hash, Mapping, RouterAddress};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    use crate::bootstrap::Bootstrap;
    use crate::destination_peers::CandidateRefusal;
    use crate::destination_tunnels::DestinationTunnelCoordinator;
    use crate::netdb_participation::PeerParticipationDriver;

    /// A validated peer record and its published version.
    struct Peer {
        record: ValidatedRouterInfo,
        hash: Hash,
    }

    /// Builds a validated peer that publishes `version`.
    fn peer(seed: u64, version: &str) -> Peer {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
        let mut options = Mapping::builder();
        options
            .insert("family".to_owned(), "ipv4".to_owned())
            .expect("family");
        options
            .insert("router.version".to_owned(), version.to_owned())
            .expect("router.version");
        let address_options = Mapping::from_entries(vec![
            ("host".to_owned(), "127.0.0.1".to_owned()),
            ("port".to_owned(), "12345".to_owned()),
        ])
        .expect("address options");
        let addresses = vec![
            RouterAddress::new(1, Date::from_millis(0), "SSU2".to_owned(), address_options)
                .expect("address"),
        ];
        let info = bundle
            .sign_router_info(
                Date::from_millis(0),
                addresses,
                Vec::new(),
                options.build().expect("options"),
            )
            .expect("signed info");
        let record = ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(0)),
        )
        .expect("validated info");
        let hash = Hash::from_bytes(*record.key().as_bytes());
        Peer { record, hash }
    }

    /// Builds the unsolicited `DatabaseStore` message that carries a peer.
    fn store_message(peer: &Peer) -> DatabaseStoreMessage {
        let encoded = peer
            .record
            .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            use std::io::Write as _;
            encoder.write_all(&encoded).expect("compress");
        }
        let compressed = encoder.finish().expect("finish");
        let compressed_len = compressed.len();
        DatabaseStoreMessage {
            key: peer.hash,
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        }
    }

    /// A real bootstrap store.
    fn bootstrap() -> Arc<StdMutex<Bootstrap>> {
        Arc::new(StdMutex::new(Bootstrap::new(
            RouterInfoStoreConfig::default(),
            crate::config::ReseedConfig {
                enabled: false,
                max_sources: 0,
                max_su3_bytes: 0,
                sources: Vec::new(),
            },
        )))
    }

    /// A real destination coordinator.
    fn destinations() -> Arc<Mutex<DestinationTunnelCoordinator>> {
        Arc::new(Mutex::new(DestinationTunnelCoordinator::new(
            i2pr_netdb::LookupPolicy::default(),
            RouterInfoStoreConfig::default(),
        )))
    }

    /// A real peer-profile handle.
    fn profiles() -> crate::netdb_participation::PeerParticipationHandle {
        crate::netdb_participation::PeerParticipationHandle::new(PeerParticipationDriver::new(
            ProfileStoreConfig::default(),
        ))
    }

    /// The reconcile under test, wired to the three real stores.
    fn reconcile(
        bootstrap: &Arc<StdMutex<Bootstrap>>,
        destinations: &Arc<Mutex<DestinationTunnelCoordinator>>,
        profiles: &crate::netdb_participation::PeerParticipationHandle,
    ) -> PeerReconcile {
        PeerReconcile::new(
            Some(StdArc::clone(bootstrap)),
            StdArc::clone(destinations),
            Some(profiles.clone()),
            MIN_RECONCILE_PERIOD_MS,
        )
        .expect("reconcile")
    }

    /// The peer keys the profile store's version-gated selection accepts.
    fn selected_peers(
        profiles: &crate::netdb_participation::PeerParticipationHandle,
    ) -> Vec<i2pr_netdb::RouterHash> {
        profiles
            .with(|driver| {
                driver
                    .select_tunnel_build_peers(8, 7)
                    .candidates
                    .iter()
                    .map(|candidate| candidate.key)
                    .collect()
            })
            .expect("profiles")
    }

    // ------------------------------------------------------------------
    // WP1 — the feasibility artefact.
    // ------------------------------------------------------------------

    /// The WP1 feasibility question, answered by running the real stores.
    ///
    /// The question: does a peer the router accepts on a live session become
    /// selectable — and insertable — without a wire change, without a new
    /// dependency, and without weakening Plan 491's gate?
    ///
    /// The answer is **yes**, and it is measured rather than argued.
    #[tokio::test]
    async fn a_live_learned_peer_becomes_selectable_and_insertable() {
        let bootstrap = bootstrap();
        let destinations = destinations();
        let profiles = profiles();
        let peer = peer(0x5060, "0.9.62");

        // The peer arrives on a live session through the real unsolicited
        // store path.
        {
            let mut guard = bootstrap.lock().expect("bootstrap lock");
            let outcome = guard
                .ingest_authenticated_router_store(&store_message(&peer), 0)
                .expect("ingest");
            assert!(
                matches!(outcome, i2pr_netdb::UnsolicitedStoreOutcome::Inserted(_)),
                "the live store must accept the peer"
            );
        }

        // Before the reconcile the peer is known only to the bootstrap.
        assert!(
            destinations
                .lock()
                .await
                .store()
                .get(&i2pr_netdb::RouterHash::from_hash(peer.hash))
                .is_none(),
            "the peer must not be in the destination store before the reconcile"
        );
        assert!(
            !selected_peers(&profiles).contains(&i2pr_netdb::RouterHash::from_hash(peer.hash)),
            "the peer must not be selectable before the reconcile"
        );

        // The reconcile runs.
        let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
        let action = reconcile.run(0).await;
        assert_eq!(
            action,
            PeerReconcileAction::Reconciled { records: 1 },
            "the reconcile must carry the live-learned peer"
        );

        // After the reconcile the peer is known to both build-with stores.
        assert!(
            destinations
                .lock()
                .await
                .store()
                .get(&i2pr_netdb::RouterHash::from_hash(peer.hash))
                .is_some(),
            "the peer must be insertable into the destination store by the reconcile"
        );
        assert!(
            selected_peers(&profiles).contains(&i2pr_netdb::RouterHash::from_hash(peer.hash)),
            "the peer must be selectable after the reconcile"
        );
    }

    /// A below-floor peer stays refused after the reconcile.
    ///
    /// Plan 491's gate lives at **selection**, not at ingestion: the
    /// reconcile writes peer records, so a peer below the floor is learned
    /// but never selected, and its refusal is counted.
    #[tokio::test]
    async fn a_below_floor_peer_stays_refused_after_the_reconcile() {
        let bootstrap = bootstrap();
        let destinations = destinations();
        let profiles = profiles();
        let peer = peer(0x5061, "0.9.61");
        {
            let mut guard = bootstrap.lock().expect("bootstrap lock");
            guard
                .ingest_authenticated_router_store(&store_message(&peer), 0)
                .expect("ingest");
        }
        let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
        reconcile.run(0).await;

        // The record is known to the stores…
        assert!(
            destinations
                .lock()
                .await
                .store()
                .get(&i2pr_netdb::RouterHash::from_hash(peer.hash))
                .is_some(),
            "the below-floor peer is still a peer record"
        );
        // …but it is never selectable.
        assert!(
            !selected_peers(&profiles).contains(&i2pr_netdb::RouterHash::from_hash(peer.hash)),
            "a below-floor peer must stay refused after the reconcile"
        );
        // The gate's refusal is counted by reason, never a silent skip.
        let refusals = profiles
            .with(|driver| driver.select_tunnel_build_peers(8, 7).refusals)
            .expect("profiles");
        assert_eq!(refusals.below_floor, 1, "the refusal must be counted");
    }

    /// The destination projection refuses the below-floor peer too.
    ///
    /// The projection applies Plan 491's gate per record, so the reconcile
    /// cannot make a below-floor peer a build candidate through the
    /// destination path either.
    #[tokio::test]
    async fn a_below_floor_peer_is_refused_by_the_destination_projection() {
        let bootstrap = bootstrap();
        let destinations = destinations();
        let profiles = profiles();
        let peer = peer(0x5062, "0.9.61");
        {
            let mut guard = bootstrap.lock().expect("bootstrap lock");
            guard
                .ingest_authenticated_router_store(&store_message(&peer), 0)
                .expect("ingest");
        }
        let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
        reconcile.run(0).await;
        let guard = destinations.lock().await;
        let refusal = crate::destination_peers::DestinationPeerCandidate::from_validated(
            guard
                .store()
                .get(&i2pr_netdb::RouterHash::from_hash(peer.hash))
                .expect("record"),
        )
        .expect_err("a below-floor peer must be refused");
        assert_eq!(refusal, CandidateRefusal::BelowVersionFloor);
    }

    // ------------------------------------------------------------------
    // The reconcile's own properties.
    // ------------------------------------------------------------------

    /// The cadence defers a second run inside the period.
    #[tokio::test]
    async fn the_cadence_defers_a_second_run_inside_the_period() {
        let bootstrap = bootstrap();
        let destinations = destinations();
        let profiles = profiles();
        let peer = peer(0x5063, "0.9.62");
        {
            let mut guard = bootstrap.lock().expect("bootstrap lock");
            guard
                .ingest_authenticated_router_store(&store_message(&peer), 0)
                .expect("ingest");
        }
        let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
        assert_eq!(
            reconcile.run(0).await,
            PeerReconcileAction::Reconciled { records: 1 }
        );
        assert_eq!(
            reconcile.run(MIN_RECONCILE_PERIOD_MS - 1).await,
            PeerReconcileAction::Deferred
        );
        assert_eq!(
            reconcile.run(MIN_RECONCILE_PERIOD_MS).await,
            PeerReconcileAction::Reconciled { records: 1 }
        );
        assert_eq!(reconcile.counters().cadence_skips, 1);
        assert_eq!(reconcile.counters().runs, 2);
    }

    /// A second reconcile of an unchanged store is idempotent and counted.
    #[tokio::test]
    async fn a_second_reconcile_is_idempotent_and_counted() {
        let bootstrap = bootstrap();
        let destinations = destinations();
        let profiles = profiles();
        let peer = peer(0x5064, "0.9.62");
        {
            let mut guard = bootstrap.lock().expect("bootstrap lock");
            guard
                .ingest_authenticated_router_store(&store_message(&peer), 0)
                .expect("ingest");
        }
        let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
        reconcile.run(0).await;
        reconcile.run(MIN_RECONCILE_PERIOD_MS).await;
        let counters = reconcile.counters();
        assert_eq!(counters.inserted, 1);
        assert_eq!(counters.idempotent, 1);
        assert_eq!(counters.learned_inserted, 1);
        assert_eq!(counters.learned_idempotent, 1);
    }

    /// A capacity refusal is counted, not fatal.
    ///
    /// The destination store refuses at capacity and never evicts, so the
    /// reconcile's second record is refused while the first one stays.
    #[tokio::test]
    async fn a_capacity_refusal_is_counted_and_not_fatal() {
        let bootstrap = bootstrap();
        let destinations = Arc::new(Mutex::new(DestinationTunnelCoordinator::new(
            i2pr_netdb::LookupPolicy::default(),
            RouterInfoStoreConfig::new(1, 64 * 1024),
        )));
        let profiles = profiles();
        let first = peer(0x5065, "0.9.62");
        let second = peer(0x5066, "0.9.62");
        {
            let mut guard = bootstrap.lock().expect("bootstrap lock");
            guard
                .ingest_authenticated_router_store(&store_message(&first), 0)
                .expect("ingest");
            guard
                .ingest_authenticated_router_store(&store_message(&second), 0)
                .expect("ingest");
        }
        let mut reconcile = reconcile(&bootstrap, &destinations, &profiles);
        let action = reconcile.run(0).await;
        assert_eq!(action, PeerReconcileAction::Reconciled { records: 2 });
        let counters = reconcile.counters();
        assert_eq!(counters.inserted, 1);
        assert_eq!(counters.capacity_exceeded, 1);
        assert_eq!(counters.conflict, 0, "a capacity refusal is not a conflict");
        // The peer-profile store is a different store with its own bound, so
        // it still learned both peers.
        assert_eq!(counters.learned_inserted, 2);
    }

    /// An unreadable bootstrap is counted, not fatal.
    #[tokio::test]
    async fn an_unreadable_source_is_counted_and_not_fatal() {
        let destinations = destinations();
        let profiles = profiles();
        let mut reconcile = PeerReconcile::new(
            None,
            StdArc::clone(&destinations),
            Some(profiles.clone()),
            MIN_RECONCILE_PERIOD_MS,
        )
        .expect("reconcile");
        assert_eq!(
            reconcile.run(0).await,
            PeerReconcileAction::SourceUnavailable
        );
        assert_eq!(reconcile.counters().source_unavailable, 1);
        assert_eq!(reconcile.counters().runs, 1);
    }

    /// An out-of-range cadence is refused at construction.
    #[test]
    fn an_out_of_range_cadence_is_refused() {
        let destinations = destinations();
        let build =
            |period_ms| PeerReconcile::new(None, StdArc::clone(&destinations), None, period_ms);
        assert!(matches!(
            build(0),
            Err(PeerReconcileError::InvalidPeriod { period_ms: 0 })
        ));
        assert!(matches!(
            build(MIN_RECONCILE_PERIOD_MS - 1),
            Err(PeerReconcileError::InvalidPeriod { .. })
        ));
        assert!(matches!(
            build(MAX_RECONCILE_PERIOD_MS + 1),
            Err(PeerReconcileError::InvalidPeriod { .. })
        ));
        assert!(build(MIN_RECONCILE_PERIOD_MS).is_ok());
        assert!(build(MAX_RECONCILE_PERIOD_MS).is_ok());
    }

    /// The counters carry stable reason labels and no peer identity.
    #[test]
    fn the_counters_carry_stable_reason_labels() {
        let counters = PeerReconcileCounters::default();
        let by_reason = counters.counts_by_reason();
        assert_eq!(by_reason.get("runs"), Some(&0));
        assert_eq!(by_reason.get("capacity-exceeded"), Some(&0));
        assert_eq!(by_reason.get("learned-capacity-exceeded"), Some(&0));
        assert!(by_reason.keys().all(|label| {
            label
                .chars()
                .all(|byte| byte.is_ascii_lowercase() || byte == '-')
        }));
    }
}
