//! Plan 488 daemon-side peer participation driver.
//!
//! The driver is the daemon's owner of the Plan 488 peer profile store. It
//! does three things and nothing else:
//!
//! - it **feeds** the store with the outcomes the router observes — a peer
//!   answered, answered without the record, refused, replied with the wrong
//!   data, did not answer, or could not be reached;
//! - it **runs the maintenance pass** on the daemon's clock, so an aged
//!   failure is demoted and then dropped instead of lingering forever;
//! - it **emits receipts** that make the observed state visible without
//!   disclosing anything about a peer.
//!
//! A receipt carries counts, tier names, state names and a digest. It never
//! carries a peer's address, its key material, its session state, or a
//! payload. A receipt that could name a peer is not a receipt.
//!
//! The driver is deliberately **not** wired into the router's tunnel
//! building in this phase. Plan 488 delivers the policy and its inputs; the
//! wiring is the consumer's change and belongs to the participation
//! phases that follow.
//!
//! Plan 497 is that consumer phase. It adds
//! [`RoutingTableConsumer`]: the daemon owns one, populates it from its own
//! NetDB, and the live lookup path asks it for the closest-first verified
//! floodfills for a key. The table stays a cache of derived state and the
//! flat peer map stays the authority, so a table miss falls back to today's
//! selection and the fallback is counted by reason.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_netdb::{
    BucketSet, BucketSetConfig, Candidate, ExplorationJob, ExplorationPolicy, ExplorationReport,
    ExplorationStop, LookupAction, LookupEngineError, LookupFinalState, LookupId, LookupKind,
    LookupPolicy, MaintenanceReport, ObserveOutcome, OptimizedPlan, ParticipationLedger,
    ProfileInsertOutcome, ProfileProjection, ProfileProjectionError, ProfileStore,
    ProfileStoreConfig, ProfileStoreStats, ReplyPath, ResponseOutcome, RotationReport, RouterHash,
    RouterInfoLookup, RouterInfoStore, RoutingTableProjectionError, SelectionConstraints,
    StartOutcome, TableInsertOutcome, ValidatedRouterInfo, ValidationContext, VersionRefusalCounts,
    daily_routing_key, plan_optimized_lookup, select_candidates, select_candidates_with_report,
};
use i2pr_proto::{
    DatabaseLookupMessage, DatabaseSearchReplyMessage, DatabaseStoreMessage, Date, Hash,
};

/// The peer-eligibility decision surface moved to [`crate::version_eligibility`]
/// in Plan 503, so the live gates and this driver share one receipt shape.
///
/// The re-export keeps Plan 491's public paths — the
/// `netdb_participation::VersionEligibilityReceipt` and
/// `netdb_participation::VersionDecisionKind` paths — unchanged for the
/// crates and the lanes that already use them.
pub use crate::version_eligibility::{VersionDecisionKind, VersionEligibilityReceipt};

/// The maximum number of receipts the driver retains.
pub const DEFAULT_MAX_RECEIPTS: usize = 256;

/// The largest maintenance period the daemon accepts.
pub const MAX_MAINTENANCE_PERIOD: Duration = Duration::from_secs(3600);

/// Plan 513: the maximum number of unresolved peers the discovery spine
/// remembers.
///
/// The spine names peers an exploration found; each named peer then needs
/// a RouterInfo-kind lookup before the router can ask it. The queue of
/// unresolved peers is bounded so a long exploration cannot make the
/// spine's memory grow without bound.
pub const MAX_PENDING_RESOLUTIONS: usize = 64;

/// The kind of event a receipt describes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParticipationReceiptKind {
    /// A peer was learned about.
    Learned,
    /// An observation was recorded against a peer.
    Observed,
    /// The maintenance pass ran.
    Maintained,
    /// A selection was drawn.
    Selected,
    /// A persisted projection was loaded or discarded.
    Projection,
}

/// A sanitized receipt describing one driver event.
///
/// The receipt carries aggregate counts, stable tier and state labels, and a
/// digest over those counts. It carries no peer identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParticipationReceipt {
    /// The kind of event this receipt describes.
    pub kind: ParticipationReceiptKind,
    /// The number of peers the store retains.
    pub peer_count: u32,
    /// The number of profiles the store retains.
    pub profile_count: u32,
    /// The number of high-capacity profiles.
    pub high_capacity_count: u32,
    /// The number of standard profiles.
    pub standard_count: u32,
    /// The number of untracked profiles.
    pub untracked_count: u32,
    /// The number of floodfill advertisers.
    pub floodfill_count: u32,
    /// The number of peers in each state, keyed by the stable state label.
    pub state_counts: BTreeMap<&'static str, u32>,
    /// The number of candidates the event selected.
    pub selected_count: u32,
    /// A digest over the receipt's counts.
    pub digest: [u8; 32],
}

impl ParticipationReceipt {
    /// Returns the stable label of the receipt kind.
    pub const fn kind_label(&self) -> &'static str {
        match self.kind {
            ParticipationReceiptKind::Learned => "learned",
            ParticipationReceiptKind::Observed => "observed",
            ParticipationReceiptKind::Maintained => "maintained",
            ParticipationReceiptKind::Selected => "selected",
            ParticipationReceiptKind::Projection => "projection",
        }
    }
}

/// The summary of a batch learn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LearnSummary {
    /// Peers newly inserted.
    pub inserted: usize,
    /// Peers whose record was byte-identical to the retained one.
    pub idempotent: usize,
    /// Peers whose strictly newer record replaced the retained one.
    pub replaced: usize,
    /// Peers whose record conflicted with the retained one.
    pub conflict: usize,
    /// Peers whose strictly older record was rejected.
    pub stale: usize,
    /// Peers refused because the underlying store was at capacity.
    pub capacity_exceeded: usize,
    /// Peers refused because the profile tier was at capacity.
    pub profile_capacity_exceeded: usize,
}

/// The outcome of loading a persisted projection.
///
/// A projection that is corrupt or oversized is **discarded and counted**.
/// It never fails the router's startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionLoadOutcome {
    /// The projection was applied.
    Restored {
        /// Profiles restored for peers the store still holds.
        restored: usize,
        /// Profiles discarded because the peer is no longer held.
        orphaned: usize,
        /// Peers the projection did not mention.
        fresh: usize,
    },
    /// The projection was discarded.
    Discarded {
        /// A bounded, human-readable reason.
        reason: &'static str,
    },
}

/// The daemon's owner of the peer profile store.
///
/// The driver keeps an explicit clock: it never reads a wall clock itself,
/// so every observation, maintenance pass and selection is reproducible.
#[derive(Debug)]
pub struct PeerParticipationDriver {
    store: ProfileStore,
    clock_ms: u64,
    last_maintenance_ms: Option<u64>,
    receipts: VecDeque<ParticipationReceipt>,
    version_receipts: VecDeque<VersionEligibilityReceipt>,
    max_receipts: usize,
    ledger: ParticipationLedger,
    discarded_projections: u32,
}

impl PeerParticipationDriver {
    /// Constructs a driver with the default receipt and participation caps.
    pub fn new(config: ProfileStoreConfig) -> Self {
        Self::with_limits(config, DEFAULT_MAX_RECEIPTS)
    }

    /// Constructs a driver with explicit receipt and participation caps.
    pub fn with_limits(config: ProfileStoreConfig, max_receipts: usize) -> Self {
        let max_participation = i2pr_netdb::DEFAULT_MAX_PARTICIPATION;
        Self {
            store: ProfileStore::new(config),
            clock_ms: 0,
            last_maintenance_ms: None,
            receipts: VecDeque::new(),
            version_receipts: VecDeque::new(),
            max_receipts,
            ledger: ParticipationLedger::new(
                max_participation,
                i2pr_netdb::MAX_PARTICIPATION_TRACKED_PEERS,
            ),
            discarded_projections: 0,
        }
    }

    /// Adopts an existing validated-RouterInfo store.
    pub fn adopt(config: ProfileStoreConfig, store: RouterInfoStore, now_ms: u64) -> Self {
        let mut driver = Self::new(config);
        driver.store = ProfileStore::adopt(config, store, now_ms);
        driver.clock_ms = now_ms;
        driver
    }

    /// Returns the underlying profile store.
    pub const fn store(&self) -> &ProfileStore {
        &self.store
    }

    /// Returns the driver's current clock reading.
    pub const fn clock_ms(&self) -> u64 {
        self.clock_ms
    }

    /// Advances the driver's clock.
    pub fn advance_time(&mut self, now_ms: u64) {
        self.clock_ms = now_ms;
    }

    /// Returns the bounded receipt log.
    ///
    /// The receipts are cloned out so a caller can hold them across a
    /// later mutation of the driver.
    pub fn receipts(&self) -> Vec<ParticipationReceipt> {
        self.receipts.iter().cloned().collect()
    }

    /// Returns the number of receipts the driver retains.
    pub fn receipt_count(&self) -> usize {
        self.receipts.len()
    }

    /// Returns the bounded log of peer-eligibility receipts.
    ///
    /// Plan 491. The log is separate from the participation receipts because
    /// it describes a *decision* — the two enforcement points — rather than
    /// the store's observed state.
    pub fn version_receipts(&self) -> Vec<VersionEligibilityReceipt> {
        self.version_receipts.iter().cloned().collect()
    }

    /// Returns the number of peer-eligibility receipts the driver retains.
    pub fn version_receipt_count(&self) -> usize {
        self.version_receipts.len()
    }

    /// Returns the number of projections the driver discarded.
    pub const fn discarded_projection_count(&self) -> u32 {
        self.discarded_projections
    }

    /// Returns the privacy-safe aggregate statistics.
    pub fn stats(&self) -> ProfileStoreStats {
        self.store.stats()
    }

    /// Learns a validated peer and records a receipt.
    pub fn learn(&mut self, validated: ValidatedRouterInfo) -> ProfileInsertOutcome {
        let outcome = self.store.insert(validated, self.clock_ms);
        self.emit_receipt(ParticipationReceiptKind::Learned, 0);
        outcome
    }

    /// Learns a batch of validated peers and records **one** receipt for the
    /// batch.
    ///
    /// A bulk import would otherwise churn the bounded receipt log with one
    /// aggregate receipt per peer.
    pub fn learn_all(
        &mut self,
        records: impl IntoIterator<Item = ValidatedRouterInfo>,
    ) -> LearnSummary {
        let mut summary = LearnSummary::default();
        for validated in records {
            match self.store.insert(validated, self.clock_ms) {
                ProfileInsertOutcome::Inserted => summary.inserted += 1,
                ProfileInsertOutcome::Idempotent => summary.idempotent += 1,
                ProfileInsertOutcome::Replaced => summary.replaced += 1,
                ProfileInsertOutcome::Conflict => summary.conflict += 1,
                ProfileInsertOutcome::StaleReplacement => summary.stale += 1,
                ProfileInsertOutcome::CapacityExceeded => summary.capacity_exceeded += 1,
                ProfileInsertOutcome::ProfileCapacityExceeded => {
                    summary.profile_capacity_exceeded += 1;
                }
            }
        }
        self.emit_receipt(ParticipationReceiptKind::Learned, 0);
        summary
    }

    /// Records that a peer answered with the requested data.
    pub fn observe_answered(&mut self, key: &RouterHash) -> ObserveOutcome {
        self.observe(key, |store, key, now| store.observe_answered(key, now))
    }

    /// Records that a peer answered but does not hold the record.
    pub fn observe_answered_empty(&mut self, key: &RouterHash) -> ObserveOutcome {
        self.observe(key, |store, key, now| {
            store.observe_answered_empty(key, now)
        })
    }

    /// Records that a peer refused or returned an error.
    pub fn observe_refused(&mut self, key: &RouterHash) -> ObserveOutcome {
        self.observe(key, |store, key, now| store.observe_refused(key, now))
    }

    /// Records that a peer replied with data that does not match the
    /// requested key.
    pub fn observe_misbehaving(&mut self, key: &RouterHash) -> ObserveOutcome {
        self.observe(key, |store, key, now| store.observe_misbehaving(key, now))
    }

    /// Records that a peer did not answer before the deadline.
    pub fn observe_unanswered(&mut self, key: &RouterHash) -> ObserveOutcome {
        self.observe(key, |store, key, now| store.observe_unanswered(key, now))
    }

    /// Records that a peer could not be reached at the transport level.
    pub fn observe_unreachable(&mut self, key: &RouterHash) -> ObserveOutcome {
        self.observe(key, |store, key, now| store.observe_unreachable(key, now))
    }

    fn observe(
        &mut self,
        key: &RouterHash,
        apply: impl FnOnce(&mut ProfileStore, &RouterHash, u64) -> ObserveOutcome,
    ) -> ObserveOutcome {
        let now = self.clock_ms;
        let outcome = apply(&mut self.store, key, now);
        self.emit_receipt(ParticipationReceiptKind::Observed, 0);
        outcome
    }

    /// Runs the maintenance pass when it is due on the driver's clock.
    ///
    /// Returns `None` when the pass is not yet due. The pass is idempotent:
    /// running it twice with the same clock changes nothing the second time.
    pub fn tick(&mut self, now_ms: u64) -> Option<MaintenanceReport> {
        self.advance_time(now_ms);
        if !self.store.maintenance_due(now_ms, self.last_maintenance_ms) {
            return None;
        }
        let report = self.store.maintain(now_ms);
        self.last_maintenance_ms = Some(now_ms);
        self.emit_receipt(ParticipationReceiptKind::Maintained, 0);
        Some(report)
    }

    /// Draws a bounded, diverse candidate set from the store.
    ///
    /// The selection records its picks in the participation ledger, so a
    /// peer that has already been selected the maximum number of times is
    /// not selected again.
    pub fn select(&mut self, count: usize, seed: u64) -> Vec<Candidate> {
        let selected = {
            let constraints = SelectionConstraints {
                count,
                seed,
                participation: self.ledger.counts(),
                ..SelectionConstraints::new(count, seed)
            };
            select_candidates(&self.store, &constraints)
        };
        for candidate in &selected {
            self.ledger.record(&candidate.key);
        }
        let selected_count = selected.len() as u32;
        self.emit_receipt(ParticipationReceiptKind::Selected, selected_count);
        selected
    }

    /// Draws the peers eligible to build a tunnel through.
    ///
    /// Plan 491. The specification's API-version table attaches "minimum
    /// peers will build tunnels through" to the level this router declares.
    /// A peer whose published API version is unknown, uncomparable, or below
    /// the floor is refused: each refusal is counted by reason and recorded
    /// in a receipt, so a refusal is never a silent skip.
    pub fn select_tunnel_build_peers(
        &mut self,
        count: usize,
        seed: u64,
    ) -> i2pr_netdb::VersionGatedSelection {
        self.select_version_gated(VersionDecisionKind::TunnelBuildPeer, count, seed, false)
    }

    /// Draws the floodfills eligible to receive a DSM.
    ///
    /// Plan 491. The specification's API-version table attaches "minimum
    /// floodfill peers will send DSM to" to the level this router declares.
    /// A floodfill whose published API version is unknown, uncomparable, or
    /// below the floor is refused and counted by reason.
    pub fn select_dsm_targets(
        &mut self,
        count: usize,
        seed: u64,
    ) -> i2pr_netdb::VersionGatedSelection {
        self.select_version_gated(VersionDecisionKind::DsmTarget, count, seed, true)
    }

    fn select_version_gated(
        &mut self,
        kind: VersionDecisionKind,
        count: usize,
        seed: u64,
        require_floodfill: bool,
    ) -> i2pr_netdb::VersionGatedSelection {
        let examined = self.store.stats().profile_count as u32;
        let selection = {
            let constraints = SelectionConstraints {
                count,
                seed,
                participation: self.ledger.counts(),
                require_floodfill,
                version_gate: Some(kind.selection_gate()),
                ..SelectionConstraints::new(count, seed)
            };
            select_candidates_with_report(&self.store, &constraints)
        };
        for candidate in &selection.candidates {
            self.ledger.record(&candidate.key);
        }
        self.emit_version_receipt(
            kind,
            examined,
            selection.candidates.len() as u32,
            &selection.refusals,
        );
        selection
    }

    /// Encodes the bounded, sanitized projection of the current state.
    pub fn encode_projection(&self) -> Result<Vec<u8>, ProfileProjectionError> {
        self.store.encode_projection()
    }

    /// Loads a persisted projection.
    ///
    /// A corrupt or oversized projection is discarded and counted; the
    /// driver never fails because of it.
    pub fn load_projection(&mut self, bytes: &[u8]) -> ProjectionLoadOutcome {
        let config = self.store.config();
        let decoded =
            ProfileProjection::decode(bytes, config.max_projection_bytes, config.max_profiles);
        let outcome = match decoded {
            Ok(projection) => {
                let report = self.store.restore(&projection);
                ProjectionLoadOutcome::Restored {
                    restored: report.restored,
                    orphaned: report.orphaned,
                    fresh: report.fresh,
                }
            }
            Err(error) => {
                self.discarded_projections = self.discarded_projections.saturating_add(1);
                ProjectionLoadOutcome::Discarded {
                    reason: projection_discard_reason(&error),
                }
            }
        };
        self.emit_receipt(ParticipationReceiptKind::Projection, 0);
        outcome
    }

    fn emit_receipt(&mut self, kind: ParticipationReceiptKind, selected_count: u32) {
        let stats = self.store.stats();
        let mut state_counts: BTreeMap<&'static str, u32> = BTreeMap::new();
        for (label, count) in &stats.state_counts {
            state_counts.insert(label, *count as u32);
        }
        let receipt = ParticipationReceipt {
            kind,
            peer_count: stats.peer_count as u32,
            profile_count: stats.profile_count as u32,
            high_capacity_count: stats.high_capacity_count as u32,
            standard_count: stats.standard_count as u32,
            untracked_count: stats.untracked_count as u32,
            floodfill_count: stats.floodfill_count as u32,
            state_counts,
            selected_count,
            digest: receipt_digest(&stats, selected_count),
        };
        if self.max_receipts == 0 {
            return;
        }
        while self.receipts.len() >= self.max_receipts {
            self.receipts.pop_front();
        }
        self.receipts.push_back(receipt);
    }

    fn emit_version_receipt(
        &mut self,
        kind: VersionDecisionKind,
        examined: u32,
        accepted: u32,
        refusals: &VersionRefusalCounts,
    ) {
        // The receipt is built by the owner module's constructor, so the
        // driver's receipts carry the same fields and the same digest as the
        // live gates' receipts.
        let receipt = VersionEligibilityReceipt::from_decision(kind, examined, accepted, refusals);
        if self.max_receipts == 0 {
            return;
        }
        while self.version_receipts.len() >= self.max_receipts {
            self.version_receipts.pop_front();
        }
        self.version_receipts.push_back(receipt);
    }
}

/// The daemon's shared handle to the participation driver.
///
/// The handle exists so a periodic owner and the observation path can reach
/// the same driver. Every critical section is short and never awaits.
#[derive(Clone, Debug)]
pub struct PeerParticipationHandle {
    inner: Arc<Mutex<PeerParticipationDriver>>,
}

impl PeerParticipationHandle {
    /// Wraps a driver in a shared handle.
    pub fn new(driver: PeerParticipationDriver) -> Self {
        Self {
            inner: Arc::new(Mutex::new(driver)),
        }
    }

    /// Runs a closure against the driver under the handle's lock.
    ///
    /// The closure must not await: the lock is held for the duration of the
    /// call and is released before the caller can suspend.
    pub fn with<R>(&self, apply: impl FnOnce(&mut PeerParticipationDriver) -> R) -> Option<R> {
        let mut guard = self.inner.lock().ok()?;
        Some(apply(&mut guard))
    }
}

/// The reason a periodic maintenance owner stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileMaintenanceExit {
    /// The caller cancelled the owner.
    Cancelled,
    /// The maintenance period was outside the accepted bound.
    InvalidPeriod,
}

/// Runs the participation maintenance pass on the daemon's clock.
///
/// The caller runs this future under the runtime's child scope and owns its
/// cancellation; there is no ownerless task. The clock is the daemon's wall
/// clock, read once per tick, and every pass is bounded by the store's own
/// limits.
pub async fn run_profile_maintenance(
    handle: PeerParticipationHandle,
    cancellation: i2pr_runtime::CancellationToken,
    maintenance_period: Duration,
) -> ProfileMaintenanceExit {
    if maintenance_period.is_zero() || maintenance_period > MAX_MAINTENANCE_PERIOD {
        return ProfileMaintenanceExit::InvalidPeriod;
    }
    let mut maintenance = tokio::time::interval(maintenance_period);
    maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return ProfileMaintenanceExit::Cancelled,
            _ = maintenance.tick() => {
                let now_ms = wall_clock_ms();
                handle.with(|driver| driver.tick(now_ms));
            }
        }
    }
}

/// Returns the daemon's wall-clock reading in milliseconds.
fn wall_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn projection_discard_reason(error: &ProfileProjectionError) -> &'static str {
    match error {
        ProfileProjectionError::TooLarge { .. } => "oversized",
        ProfileProjectionError::Malformed { .. } => "malformed",
        ProfileProjectionError::DigestMismatch => "digest-mismatch",
        ProfileProjectionError::TooManyRecords { .. } => "too-many-records",
        ProfileProjectionError::ScoreOutOfRange { .. } => "score-out-of-range",
        ProfileProjectionError::UnknownLabel { .. } => "unknown-label",
    }
}

fn receipt_digest(stats: &ProfileStoreStats, selected_count: u32) -> [u8; 32] {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(stats.peer_count.to_string().as_bytes());
    buffer.push(b'|');
    buffer.extend_from_slice(stats.profile_count.to_string().as_bytes());
    buffer.push(b'|');
    buffer.extend_from_slice(stats.high_capacity_count.to_string().as_bytes());
    buffer.push(b'|');
    buffer.extend_from_slice(stats.standard_count.to_string().as_bytes());
    buffer.push(b'|');
    buffer.extend_from_slice(stats.untracked_count.to_string().as_bytes());
    buffer.push(b'|');
    buffer.extend_from_slice(stats.floodfill_count.to_string().as_bytes());
    buffer.push(b'|');
    buffer.extend_from_slice(selected_count.to_string().as_bytes());
    buffer.push(b'|');
    for (label, count) in &stats.state_counts {
        buffer.extend_from_slice(label.as_bytes());
        buffer.push(b'=');
        buffer.extend_from_slice(count.to_string().as_bytes());
        buffer.push(b',');
    }
    let hash = Hash::digest(&buffer);
    let mut digest = [0u8; 32];
    digest.copy_from_slice(hash.as_bytes());
    digest
}

/// The daemon-side consumer seam for the floodfill-indexed routing table.
///
/// Plan 497. Plans 488 and 489 delivered the peer-profile store, the
/// floodfill-indexed routing table and the optimized lookup plan, and both
/// recorded that nothing in the daemon called them. This seam is that
/// consumer's surface: the daemon owns one, populates it from its own NetDB,
/// and the live lookup path asks it for the closest-first verified floodfills
/// for a key.
///
/// The table is a **cache of derived state**; the flat peer map stays the
/// **authority**. A table that cannot answer a key never fails the lookup —
/// the caller falls back to the flat store's selection, and every fallback is
/// counted by reason. The table may only ever improve the choice, never
/// remove a candidate the flat store could serve.
pub struct RoutingTableConsumer {
    inner: Arc<Mutex<RoutingTableConsumerState>>,
    /// Lookups whose candidate list came from the table.
    ///
    /// The counters live **outside** the lock on purpose. A poisoned
    /// table cannot answer, and that fallback must still be counted by
    /// reason: a fallback that is not counted is a silent fallback.
    answered: AtomicU64,
    /// Lookups that fell back because the table is disabled.
    fell_back_disabled: AtomicU64,
    /// Lookups that fell back because the table held no queryable candidate
    /// for the key.
    fell_back_no_candidate: AtomicU64,
    /// Lookups that fell back because the table could not be reached at
    /// all — a poisoned lock.
    fell_back_unavailable: AtomicU64,
}

/// The consumer's state: the driver and the operator's switch.
struct RoutingTableConsumerState {
    /// Whether the table may answer a lookup at all.
    enabled: bool,
    /// The Plan 489 driver.
    driver: RoutingTableDriver,
}

/// The counted outcome of asking the routing table for candidates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RoutingTableCounters {
    /// Lookups whose candidate list came from the routing table.
    pub answered: u64,
    /// Lookups that fell back to the flat store because the table is off.
    pub fell_back_disabled: u64,
    /// Lookups that fell back to the flat store because the table held no
    /// queryable candidate for the key.
    pub fell_back_no_candidate: u64,
    /// Lookups that fell back to the flat store because the table could not
    /// be reached at all.
    pub fell_back_unavailable: u64,
}

impl RoutingTableCounters {
    /// Returns the number of lookups that fell back to the flat store.
    pub const fn fell_back(&self) -> u64 {
        self.fell_back_disabled
            .saturating_add(self.fell_back_no_candidate)
            .saturating_add(self.fell_back_unavailable)
    }
}

/// The answer to a candidate request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingTableCandidates {
    /// The table answered with this closest-first candidate list.
    Table(Vec<RouterHash>),
    /// The table could not answer; the caller must use the flat store.
    Fallback(RoutingTableFallback),
}

/// Why the routing table could not answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingTableFallback {
    /// The table is disabled.
    Disabled,
    /// The table holds no queryable candidate for the key.
    NoCandidate,
    /// The table could not be reached at all — its lock is poisoned.
    ///
    /// The fallback is counted under this reason rather than under
    /// [`Self::NoCandidate`]: an operator reading the counters must be
    /// able to tell "the table had no peer for this key" apart from
    /// "the table is broken".
    Unavailable,
}

impl RoutingTableConsumer {
    /// Wraps a driver in a shared consumer.
    ///
    /// `enabled` is the operator's switch. A disabled consumer answers
    /// every request with [`RoutingTableFallback::Disabled`], so the
    /// caller falls back to the flat store exactly as it does today.
    pub fn new(driver: RoutingTableDriver, enabled: bool) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RoutingTableConsumerState { enabled, driver })),
            answered: AtomicU64::new(0),
            fell_back_disabled: AtomicU64::new(0),
            fell_back_no_candidate: AtomicU64::new(0),
            fell_back_unavailable: AtomicU64::new(0),
        }
    }

    /// Returns whether the table may answer a lookup.
    pub fn is_enabled(&self) -> bool {
        self.inner
            .lock()
            .map(|guard| guard.enabled)
            .unwrap_or(false)
    }

    /// Indexes one validated peer, recording its own signed evidence.
    pub fn index(&self, validated: &ValidatedRouterInfo) -> Option<TableInsertOutcome> {
        self.inner
            .lock()
            .ok()
            .map(|mut guard| guard.driver.index(validated))
    }

    /// Indexes one peer a verified floodfill suggested.
    pub fn index_suggested(
        &self,
        key: RouterHash,
        from: &RouterHash,
    ) -> Option<TableInsertOutcome> {
        self.inner
            .lock()
            .ok()
            .map(|mut guard| guard.driver.index_suggested(key, from))
    }

    /// Advances the driver's clock so the daily rotation can happen.
    pub fn advance_time(&self, now_ms: u64) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.driver.advance_time(now_ms);
        }
    }

    /// Returns the table's **evidence split**: the peers it holds with
    /// signed evidence and the peers it holds only as suggestions.
    ///
    /// The split is what distinguishes a peer the router may use as
    /// evidence from a peer it merely heard about. A poisoned table
    /// reports no peers at all: a caller must never see evidence it
    /// cannot vouch for.
    pub fn evidence_counts(&self) -> (usize, usize) {
        self.inner
            .lock()
            .map(|guard| guard.driver.table().evidence_counts())
            .unwrap_or((0, 0))
    }

    /// Returns whether the table holds the peer.
    pub fn contains(&self, key: &RouterHash) -> bool {
        self.inner
            .lock()
            .map(|guard| guard.driver.table().contains(key))
            .unwrap_or(false)
    }

    /// Returns the number of peers the table currently holds.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .map(|guard| guard.driver.table().len())
            .unwrap_or(0)
    }

    /// Returns whether the table holds no peer.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Asks the table for the closest-first verified floodfills for a key.
    ///
    /// The returned list is the table's answer. A [`RoutingTableFallback`]
    /// means the caller must use the flat store's selection; the reason is
    /// counted either way.
    pub fn candidates(
        &self,
        store: &RouterInfoStore,
        routing_key: &RouterHash,
        excluded: &[RouterHash],
    ) -> RoutingTableCandidates {
        let Ok(guard) = self.inner.lock() else {
            // A poisoned table cannot answer. The fallback is still
            // counted, by its own reason: a fallback that is not
            // counted is a silent fallback.
            self.fell_back_unavailable.fetch_add(1, Ordering::Relaxed);
            return RoutingTableCandidates::Fallback(RoutingTableFallback::Unavailable);
        };
        if !guard.enabled {
            self.fell_back_disabled.fetch_add(1, Ordering::Relaxed);
            return RoutingTableCandidates::Fallback(RoutingTableFallback::Disabled);
        }
        let plan = guard.driver.plan(store, *routing_key, excluded);
        if plan.queryable.is_empty() {
            self.fell_back_no_candidate.fetch_add(1, Ordering::Relaxed);
            return RoutingTableCandidates::Fallback(RoutingTableFallback::NoCandidate);
        }
        self.answered.fetch_add(1, Ordering::Relaxed);
        RoutingTableCandidates::Table(plan.queryable_hashes())
    }

    /// Returns the counted outcomes.
    pub fn counters(&self) -> RoutingTableCounters {
        RoutingTableCounters {
            answered: self.answered.load(Ordering::Relaxed),
            fell_back_disabled: self.fell_back_disabled.load(Ordering::Relaxed),
            fell_back_no_candidate: self.fell_back_no_candidate.load(Ordering::Relaxed),
            fell_back_unavailable: self.fell_back_unavailable.load(Ordering::Relaxed),
        }
    }

    /// Plan 513: sets the reply path the consumer's requests carry.
    ///
    /// The reply path is the **inbound** exploratory tunnel: the answer
    /// to an exploratory request and to a peer resolution travels it. The
    /// request itself travels the **outbound** exploratory tunnel, which
    /// the caller composes.
    ///
    /// Without a reply path the lookup state machines refuse to emit a
    /// `DatabaseLookup`: a direct peer link is not a reply path.
    pub fn set_reply_path(&self, path: Option<ReplyPath>) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.driver.set_reply_path(path);
        }
    }

    /// Plan 513: starts one exploration on the consumer's private driver.
    ///
    /// The driver is private inside the consumer's own state, so this is
    /// the production entry point that reaches it. The request is an
    /// exploratory `DatabaseLookup` — wire lookup type `3` — whose key is
    /// the supplied random search key, and the dispatch names the peer the
    /// exploration asks.
    ///
    /// A table that cannot be reached is counted under
    /// [`RoutingTableFallback::Unavailable`]: the exploration terminates
    /// honestly rather than silently doing nothing.
    ///
    /// Plan 514: the peer the exploration asks is the routing table's
    /// **closest** verified floodfill for the search key's routing key.
    /// The flat store's iteration order is not a selection: it named the
    /// same peer for every exploration, so a peer that did not answer was
    /// asked again and again. The reference asks the same question of its
    /// own floodfill bucket set (`libi2pd/NetDbRequests.cpp:479`, at the
    /// exact pin).
    ///
    /// A table that names **no** floodfill ends the request, counted under
    /// [`NetDbDiscoveryCounters::explorations_refused_no_target`]. The
    /// request is never redirected to an arbitrary substitute — the
    /// reference ends its request too, rather than asking one
    /// (`libi2pd/NetDbRequests.cpp:502-503`).
    pub fn begin_exploration(
        &self,
        store: &RouterInfoStore,
        search_key: RouterHash,
        routing_key: RouterHash,
    ) -> RoutingDispatch {
        let Ok(mut guard) = self.inner.lock() else {
            self.fell_back_unavailable.fetch_add(1, Ordering::Relaxed);
            return RoutingDispatch::Terminal {
                lookup_id: LookupId::new(0, LookupKind::Exploratory, search_key),
                final_state: LookupFinalState::Cancelled,
            };
        };
        guard
            .driver
            .begin_exploration(store, search_key, routing_key)
    }

    /// Plan 513: advances the active exploration to its next peer.
    ///
    /// The exploration's peer budget and deadline gate the advance, exactly
    /// as they gate the driver's own advance.
    pub fn exploration_next(&self, store: &RouterInfoStore) -> RoutingDispatch {
        let Ok(mut guard) = self.inner.lock() else {
            self.fell_back_unavailable.fetch_add(1, Ordering::Relaxed);
            return RoutingDispatch::Terminal {
                lookup_id: LookupId::new(
                    0,
                    LookupKind::Exploratory,
                    RouterHash::from_bytes([0; 32]),
                ),
                final_state: LookupFinalState::Cancelled,
            };
        };
        guard.driver.exploration_next(store)
    }

    /// Plan 513: ingests one exploration reply.
    ///
    /// The reply's peers are offered to the routing table as
    /// **suggestions**. A suggestion is never evidence: the suggesting peer
    /// must itself be indexed with signed evidence, or the table refuses
    /// the suggestion.
    pub fn ingest_search_reply(
        &self,
        lookup_id: LookupId,
        reply: &DatabaseSearchReplyMessage,
    ) -> Result<RoutingDiscovery, LookupEngineError> {
        let Ok(mut guard) = self.inner.lock() else {
            self.fell_back_unavailable.fetch_add(1, Ordering::Relaxed);
            return Err(LookupEngineError::LookupTerminal);
        };
        guard.driver.ingest_search_reply(lookup_id, reply)
    }

    /// Plan 513: the peer-resolution path.
    ///
    /// This answers [`RoutingLookupOutcome::NeedsPeerResolution`]: the
    /// table holds peers whose records the router has not resolved, so the
    /// router asks a floodfill it already knows for one of those peers'
    /// `RouterInfo`.
    ///
    /// The candidates come from the **authoritative flat store**, never from
    /// the table: the table's pending peers are exactly the ones the router
    /// cannot reach yet, so asking the table for the candidates to resolve
    /// them would never terminate.
    pub fn resolve_peer(
        &self,
        store: &RouterInfoStore,
        target: RouterHash,
        routing_key: RouterHash,
    ) -> RoutingLookupOutcome {
        let Ok(mut guard) = self.inner.lock() else {
            self.fell_back_unavailable.fetch_add(1, Ordering::Relaxed);
            return RoutingLookupOutcome::Terminal {
                lookup_id: LookupId::new(0, LookupKind::RouterInfo, target),
                final_state: LookupFinalState::Cancelled,
            };
        };
        guard
            .driver
            .begin_peer_resolution(store, target, routing_key)
    }

    /// Plan 513: the exploration budget and deadline policy the consumer's
    /// private driver was configured with.
    pub fn exploration_policy(&self) -> ExplorationPolicy {
        self.inner
            .lock()
            .map(|guard| guard.driver.exploration_policy)
            .unwrap_or_default()
    }

    /// Plan 514: the routing table's **closest** verified floodfill.
    ///
    /// The answer is the table's closest-first selection for the key,
    /// restricted to the peers that advertise `f` and that the flat store
    /// holds. The reference asks the same question of its own floodfill
    /// bucket set (`libi2pd/NetDbRequests.cpp:479`, at the exact pin).
    ///
    /// Unlike [`Self::preferred_peers`], the answer is a **target**, not a
    /// preference: the caller asks this peer or asks nobody. A table that
    /// cannot answer yields `None`, and the reason is counted by reason —
    /// `Disabled`, `NoCandidate` or `Unavailable` — exactly as a
    /// preference's fallback is. A table that cannot answer never sends the
    /// caller to an arbitrary substitute.
    pub fn closest_floodfill(
        &self,
        store: &RouterInfoStore,
        routing_key: &RouterHash,
        excluded: &[RouterHash],
    ) -> Option<RouterHash> {
        match self.candidates(store, routing_key, excluded) {
            RoutingTableCandidates::Table(candidates) => candidates.into_iter().next(),
            RoutingTableCandidates::Fallback(_) => None,
        }
    }

    /// Plan 513: the routing table's **preference** over a candidate list.
    ///
    /// The returned list is the table's closest-first answer. The caller
    /// hands it to the lookup state machine's preferred start, which
    /// **reorders** the flat store's own selection and never narrows it: a
    /// peer the flat store could serve is still served, and a peer the flat
    /// store would not have served is ignored.
    ///
    /// A table that cannot answer yields an empty preference and the
    /// fallback is counted by reason — the flat store's own order then
    /// stands. A preference is never a silent fallback.
    pub fn preferred_peers(
        &self,
        store: &RouterInfoStore,
        routing_key: &RouterHash,
        excluded: &[RouterHash],
    ) -> Vec<RouterHash> {
        match self.candidates(store, routing_key, excluded) {
            RoutingTableCandidates::Table(preferred) => preferred,
            RoutingTableCandidates::Fallback(_) => Vec::new(),
        }
    }
}

/// Plan 513: how one discovery request must be delivered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryDelivery {
    /// The request rides the **outbound** exploratory tunnel to the named
    /// peer, and the reply travels the **inbound** exploratory tunnel.
    ///
    /// This is the only delivery a RouterInfo-kind lookup may take: Plan
    /// 102's authority amendment forbids substituting a direct peer link
    /// for the exploratory tunnel.
    OutboundExploratoryTunnel,
    /// The request is delivered **directly** to the named floodfill.
    ///
    /// This is the only delivery an exploratory lookup may take: the
    /// specification addresses an exploratory request straight at the
    /// floodfill being asked.
    Direct,
}

/// Plan 513: one discovery request the spine has composed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryDispatch {
    /// The request must reach the named peer.
    Send {
        /// The lookup identity the request belongs to.
        lookup_id: LookupId,
        /// The peer the request is addressed to.
        peer: RouterHash,
        /// The request body.
        message: DatabaseLookupMessage,
        /// How the request must be delivered.
        delivery: DiscoveryDelivery,
    },
    /// The request cannot be composed yet: the spine has no reply path.
    NeedsReplyPath {
        /// The lookup identity the request belongs to.
        lookup_id: LookupId,
    },
    /// The request is terminal.
    Terminal {
        /// The lookup identity the request belongs to.
        lookup_id: LookupId,
        /// Why the request ended.
        final_state: LookupFinalState,
    },
}

/// Plan 513: the counted outcomes of the live discovery spine.
///
/// Every refusal is counted by reason. A fallback that is not counted here
/// is a defect, not a fallback.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NetDbDiscoveryCounters {
    /// Exploratory requests the spine composed.
    pub explorations_started: u64,
    /// Peer-resolution requests the spine composed.
    pub resolutions_started: u64,
    /// Exploratory replies the spine ingested.
    pub search_replies_ingested: u64,
    /// Named peers the spine offered to the routing table as suggestions.
    pub suggestions_offered: u64,
    /// Named peers the routing table refused as suggestions.
    pub suggestions_refused: u64,
    /// Requests whose candidate preference fell back to the flat store's
    /// own selection because the routing table was off.
    pub fell_back_disabled: u64,
    /// Requests whose candidate preference fell back to the flat store's
    /// own selection because the table held no candidate.
    pub fell_back_no_candidate: u64,
    /// Requests whose candidate preference fell back to the flat store's
    /// own selection because the table could not be reached.
    pub fell_back_unavailable: u64,
    /// Requests the spine could not compose because the router holds no
    /// inbound exploratory tunnel to take the reply.
    pub fell_back_no_reply_path: u64,
    /// Requests the transport accepted and carried to the wire.
    pub requests_delivered: u64,
    /// Requests the transport refused, or the delivery could not be
    /// composed at all.
    pub delivery_failures: u64,
    /// Signed records a RouterInfo-kind lookup returned and the router
    /// ingested into its NetDB.
    pub records_ingested: u64,
    /// Explorations the wire never answered before the exploration's own
    /// deadline passed.
    pub explorations_expired: u64,
    /// RouterInfo-kind lookups the wire never answered before the lookup
    /// policy's total deadline passed.
    pub resolutions_expired: u64,
    /// Plan 514: exploratory requests the routing table named **no**
    /// floodfill to ask.
    ///
    /// The request is refused when this counter moves. It is never
    /// redirected to an arbitrary peer: a router that asks somebody it did
    /// not choose is not exploring, it is probing.
    pub explorations_refused_no_target: u64,
}

/// Plan 513: the live NetDB discovery spine.
///
/// One bounded owner of the router's DHT discovery:
///
/// 1. the **exploratory lookup** — wire type `3` — that asks one named
///    floodfill for the peers closest to a random search key;
/// 2. the **RouterInfo-kind lookup** — wire type `2` — that resolves a
///    named peer into a `RouterInfo`.
///
/// The spine owns no task, no timer, no listener and no socket. The
/// daemon's existing tick drives it. Every refusal it records is counted
/// by reason.
pub struct NetDbDiscoverySpine {
    inner: Mutex<NetDbDiscoveryState>,
}

/// The spine's mutable state.
struct NetDbDiscoveryState {
    /// The live seam: the RouterInfo-kind and Exploratory-kind starts.
    seam: crate::netdb_seam::NetDbSeam,
    /// The routing table: the candidate preference and the suggestion sink.
    table: Arc<RoutingTableConsumer>,
    /// The active exploration's budget authority.
    exploration: Option<ExplorationState>,
    /// The counted outcomes.
    counters: NetDbDiscoveryCounters,
    /// The next request identity the spine allocates.
    next_request_id: u64,
    /// The moment the spine last advanced, in milliseconds.
    now_ms: u64,
    /// The inbound exploratory tunnel's reply path, refreshed by the
    /// owner that holds the exploratory pool. The reply to every request
    /// the spine composes travels it.
    reply_path: Option<ReplyPath>,
    /// The request the spine composed and the transport owner has not
    /// delivered yet.
    ///
    /// The slot holds **one** request. A request that is still waiting when
    /// the next request starts is dropped: the spine never queues without
    /// bound.
    dispatch: Option<DiscoveryDispatch>,
    /// The named peers an exploration named that the router has not
    /// resolved yet.
    ///
    /// A named peer is a **name**, not a peer: the router cannot ask it
    /// until a RouterInfo-kind lookup has returned its signed record. The
    /// queue is bounded by [`MAX_PENDING_RESOLUTIONS`].
    pending_resolutions: VecDeque<RouterHash>,
    /// The peer the active RouterInfo-kind lookup is resolving.
    ///
    /// One resolution at a time: the router asks one peer at a time, so the
    /// request it owes the wire is never ambiguous.
    resolving: Option<RouterHash>,
    /// The moment the active RouterInfo-kind lookup expires, in
    /// milliseconds.
    ///
    /// A lookup the wire never answered is abandoned when its deadline
    /// passes, so one unanswered request cannot stall the discovery cycle
    /// forever.
    resolving_expires_ms: u64,
}

/// Plan 513: the active exploration's bounded state.
struct ExplorationState {
    /// The peer budget and deadline authority.
    job: ExplorationJob,
    /// The peer the exploration asked. Only this peer's reply may
    /// introduce new peers.
    asked: Option<RouterHash>,
}

/// Plan 513: the outcome of one ingested exploratory reply.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiscoveryOutcome {
    /// The named peers the reply carried.
    pub suggested: usize,
    /// The named peers the routing table accepted as suggestions.
    pub indexed: usize,
    /// The named peers the routing table refused as suggestions.
    pub refused: usize,
    /// The named peers the reply's ceiling dropped.
    pub dropped: usize,
    /// `true` when the reply came from a peer the exploration never asked.
    pub unknown_peer: bool,
    /// `true` when the ingested record completed a RouterInfo-kind lookup.
    pub resolved: bool,
}

/// Plan 513: the failure modes of the live discovery spine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    /// No exploratory lookup is active, so the reply has nowhere to go.
    NoActiveExploration,
    /// The live seam refused the reply.
    Seam(crate::netdb_seam::NetDbSeamError),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoActiveExploration => formatter.write_str("no active exploration"),
            Self::Seam(error) => write!(formatter, "netdb seam: {error}"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

impl NetDbDiscoverySpine {
    /// Constructs a spine over the shared routing table.
    pub fn new(table: Arc<RoutingTableConsumer>, policy: LookupPolicy) -> Self {
        Self {
            inner: Mutex::new(NetDbDiscoveryState {
                seam: crate::netdb_seam::NetDbSeam::new(policy),
                table,
                exploration: None,
                counters: NetDbDiscoveryCounters::default(),
                next_request_id: 1,
                now_ms: 0,
                reply_path: None,
                dispatch: None,
                pending_resolutions: VecDeque::new(),
                resolving: None,
                resolving_expires_ms: 0,
            }),
        }
    }

    /// Plan 513: records the inbound exploratory tunnel's reply path.
    ///
    /// The reply to every request the spine composes travels this tunnel.
    /// The owner that holds the exploratory pool refreshes it; `None` means
    /// the router holds no inbound exploratory tunnel, and every request
    /// the spine composes is then refused and counted.
    pub fn set_reply_path(&self, path: Option<ReplyPath>) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.reply_path = path;
        }
    }

    /// Plan 513: returns the inbound exploratory tunnel's reply path.
    pub fn reply_path(&self) -> Option<ReplyPath> {
        self.inner.lock().ok().and_then(|guard| guard.reply_path)
    }

    /// Plan 513: records the request the transport owner has not delivered
    /// yet.
    ///
    /// The slot holds **one** request. A request that is still waiting when
    /// the next request is recorded is dropped: the spine never queues
    /// without bound.
    pub fn set_dispatch(&self, dispatch: DiscoveryDispatch) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.dispatch = Some(dispatch);
        }
    }

    /// Plan 513: returns the request the transport owner has not delivered
    /// yet, and clears the slot.
    pub fn take_dispatch(&self) -> Option<DiscoveryDispatch> {
        self.inner
            .lock()
            .ok()
            .and_then(|mut guard| guard.dispatch.take())
    }

    /// Advances the spine's clock to `now_ms`.
    ///
    /// The spine has no timer of its own: the daemon's existing tick calls
    /// this.
    ///
    /// A request the wire never answered is abandoned here when its
    /// deadline passes: the exploration's own deadline for an exploration,
    /// and the lookup policy's total deadline for a RouterInfo-kind
    /// lookup. An abandoned request is counted, never retried in a loop,
    /// and the discovery cycle moves on.
    pub fn advance_time(&self, now_ms: u64) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        guard.now_ms = now_ms;
        let exploration_expired = guard
            .exploration
            .as_ref()
            .is_some_and(|exploration| exploration.job.is_expired(now_ms));
        if exploration_expired {
            guard.exploration = None;
            guard.counters.explorations_expired += 1;
        }
        let resolution_expired = guard.resolving.is_some() && now_ms >= guard.resolving_expires_ms;
        if resolution_expired {
            guard.resolving = None;
            guard.resolving_expires_ms = 0;
            guard.counters.resolutions_expired += 1;
        }
    }

    /// Returns the counted outcomes.
    pub fn counters(&self) -> NetDbDiscoveryCounters {
        self.inner
            .lock()
            .map(|guard| guard.counters)
            .unwrap_or_default()
    }

    /// Plan 513: advances the seam's RouterInfo-kind validation clock.
    ///
    /// A RouterInfo record is validated against the clock the caller
    /// supplies. A stale clock would refuse every record the router
    /// resolves, so the transport owner advances it on every tick.
    pub fn set_routerinfo_now_seconds(&self, now_seconds: u32) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.seam.set_routerinfo_now_seconds(now_seconds);
        }
    }

    /// Plan 513: records the outcome of one delivery attempt.
    ///
    /// The transport owner calls this after it hands a composed request to
    /// the transport. A refusal is counted: a request that never reached
    /// the wire is not a delivered request, and the difference must be
    /// visible.
    pub fn record_delivery(&self, delivered: bool) {
        if let Ok(mut guard) = self.inner.lock() {
            if delivered {
                guard.counters.requests_delivered =
                    guard.counters.requests_delivered.saturating_add(1);
            } else {
                guard.counters.delivery_failures =
                    guard.counters.delivery_failures.saturating_add(1);
            }
        }
    }

    /// Plan 513: returns `true` while an exploration awaits its reply.
    ///
    /// One exploration at a time: a new exploration replaces a finished
    /// one, never a running one. The router therefore never owes the wire
    /// two exploratory requests at once.
    pub fn exploration_in_flight(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .map(|guard| guard.exploration.is_some())
            .unwrap_or(false)
    }

    /// Plan 513: returns `true` while a RouterInfo-kind lookup awaits its
    /// reply.
    pub fn resolution_in_flight(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .map(|guard| guard.resolving.is_some())
            .unwrap_or(false)
    }

    /// Plan 513: returns the named peer the router should resolve next.
    ///
    /// The peer is one an exploration named and the router has not yet
    /// resolved. A peer already in the store is not returned: the router
    /// does not ask for a record it already holds.
    pub fn pending_resolution_target(&self, store: &RouterInfoStore) -> Option<RouterHash> {
        let guard = self.inner.lock().ok()?;
        guard
            .pending_resolutions
            .iter()
            .copied()
            .find(|peer| !store.contains(peer))
    }

    /// Plan 513: the unresolved peers the spine remembers.
    pub fn pending_resolutions(&self) -> Vec<RouterHash> {
        self.inner
            .lock()
            .map(|guard| guard.pending_resolutions.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Plan 513: starts the **exploratory lookup** against the named
    /// floodfill.
    ///
    /// The request is an exploratory `DatabaseLookup` — wire lookup type
    /// `3` — whose key is the supplied random search key. The named
    /// floodfill is the peer being asked, and the request is delivered
    /// **directly** to it, which is the specification's path for an
    /// exploratory lookup.
    ///
    /// The exploration's peer budget and deadline gate the request. The
    /// reply travels the **inbound** exploratory tunnel `reply_path` names;
    /// without one the request is refused and counted.
    pub fn begin_exploration(
        &self,
        store: &RouterInfoStore,
        search_key: RouterHash,
        routing_key: RouterHash,
        reply_path: Option<ReplyPath>,
    ) -> DiscoveryDispatch {
        let Ok(mut guard) = self.inner.lock() else {
            return DiscoveryDispatch::Terminal {
                lookup_id: LookupId::new(0, LookupKind::Exploratory, search_key),
                final_state: LookupFinalState::Cancelled,
            };
        };
        let now_ms = guard.now_ms;
        // Plan 514: the peer the exploration asks is the routing table's
        // **closest** verified floodfill for the search key's routing key.
        // The flat store's iteration order is not a selection -- it named
        // the same peer for every exploration.
        //
        // A table that names no floodfill ends the request, counted. The
        // request is never redirected to an arbitrary substitute.
        let Some(floodfill) = guard.table.closest_floodfill(store, &routing_key, &[]) else {
            guard.counters.explorations_refused_no_target += 1;
            return DiscoveryDispatch::Terminal {
                lookup_id: LookupId::new(0, LookupKind::Exploratory, search_key),
                final_state: LookupFinalState::NoEligibleCandidates,
            };
        };
        // The exploration's budget and deadline gate the request.
        let job = ExplorationJob::begin(
            search_key,
            routing_key,
            guard.table.exploration_policy(),
            now_ms,
        );
        if job.budget_exhausted() || job.is_expired(now_ms) {
            guard.counters.fell_back_no_candidate += 1;
            return DiscoveryDispatch::Terminal {
                lookup_id: LookupId::new(0, LookupKind::Exploratory, search_key),
                final_state: LookupFinalState::NoEligibleCandidates,
            };
        }
        let request_id = guard.next_request_id;
        guard.next_request_id = guard.next_request_id.saturating_add(1);
        let reply_path = reply_path.or(guard.reply_path);
        let action = guard
            .seam
            .begin_exploratory_lookup(request_id, search_key, floodfill, reply_path);
        let dispatch = discovery_dispatch_from_action(action, DiscoveryDelivery::Direct);
        if let DiscoveryDispatch::Send { peer, .. } = &dispatch {
            let mut job = job;
            job.record_issued(peer);
            guard.exploration = Some(ExplorationState {
                job,
                asked: Some(*peer),
            });
            guard.counters.explorations_started += 1;
        } else if let DiscoveryDispatch::NeedsReplyPath { .. } = &dispatch {
            guard.counters.fell_back_no_reply_path += 1;
        }
        dispatch
    }

    /// Plan 513: starts the **RouterInfo-kind lookup** that resolves the
    /// named peer.
    ///
    /// The request is a RouterInfo `DatabaseLookup` — wire lookup type `2`
    /// — and it rides the **outbound** exploratory tunnel. The reply
    /// travels the **inbound** exploratory tunnel `reply_path` names.
    ///
    /// The candidate selection is the flat store's own selection, reordered
    /// by the routing table's answer. The table may only **reorder** that
    /// selection: it never narrows it and never adds a peer the flat store
    /// would not have served.
    pub fn begin_peer_resolution(
        &self,
        store: &RouterInfoStore,
        target: RouterHash,
        routing_key: RouterHash,
        reply_path: Option<ReplyPath>,
    ) -> DiscoveryDispatch {
        let Ok(mut guard) = self.inner.lock() else {
            return DiscoveryDispatch::Terminal {
                lookup_id: LookupId::new(0, LookupKind::RouterInfo, target),
                final_state: LookupFinalState::Cancelled,
            };
        };
        // The table's answer is a preference, never a replacement. A
        // fallback is counted by reason: the request still starts, on the
        // flat store's own selection, but the fallback is never silent.
        let preferred = match guard.table.candidates(store, &routing_key, &[]) {
            RoutingTableCandidates::Table(preferred) => preferred,
            RoutingTableCandidates::Fallback(reason) => {
                match reason {
                    RoutingTableFallback::Disabled => {
                        guard.counters.fell_back_disabled =
                            guard.counters.fell_back_disabled.saturating_add(1);
                    }
                    RoutingTableFallback::NoCandidate => {
                        guard.counters.fell_back_no_candidate =
                            guard.counters.fell_back_no_candidate.saturating_add(1);
                    }
                    RoutingTableFallback::Unavailable => {
                        guard.counters.fell_back_unavailable =
                            guard.counters.fell_back_unavailable.saturating_add(1);
                    }
                }
                Vec::new()
            }
        };
        let request_id = guard.next_request_id;
        guard.next_request_id = guard.next_request_id.saturating_add(1);
        let reply_path = reply_path.or(guard.reply_path);
        let action = guard.seam.begin_routerinfo_lookup(
            store,
            request_id,
            target,
            &routing_key,
            Some(&preferred),
            reply_path,
        );
        let dispatch =
            discovery_dispatch_from_action(action, DiscoveryDelivery::OutboundExploratoryTunnel);
        match &dispatch {
            DiscoveryDispatch::Send { .. } => {
                guard.counters.resolutions_started += 1;
                // One resolution at a time: the peer this lookup is
                // resolving is remembered until its record arrives, and
                // the lookup is abandoned when its deadline passes.
                guard.resolving = Some(target);
                guard.resolving_expires_ms = guard
                    .now_ms
                    .saturating_add(guard.seam.policy().total_deadline_ms());
            }
            DiscoveryDispatch::NeedsReplyPath { .. } => {
                guard.counters.fell_back_no_reply_path += 1;
            }
            DiscoveryDispatch::Terminal { .. } => guard.counters.fell_back_no_candidate += 1,
        }
        dispatch
    }

    /// Plan 513: ingests one `DatabaseStore` into the active RouterInfo-kind
    /// lookup.
    ///
    /// The record is validated by the lookup engine's own rules and
    /// inserted into the router's NetDB store, so the named peer becomes a
    /// peer the router can ask. The peer leaves the unresolved queue only
    /// when its record actually arrived: a store the router already holds
    /// is not re-resolved, and a record that failed validation changes
    /// nothing.
    pub fn ingest_routerinfo_store(
        &self,
        store: &mut RouterInfoStore,
        store_message: &DatabaseStoreMessage,
    ) -> Result<DiscoveryOutcome, DiscoveryError> {
        let Ok(mut guard) = self.inner.lock() else {
            return Err(DiscoveryError::NoActiveExploration);
        };
        let target = guard.resolving;
        let outcome = guard
            .seam
            .ingest_routerinfo_store(store, store_message)
            .map_err(DiscoveryError::Seam)?;
        let resolved = matches!(outcome, i2pr_netdb::ResponseOutcome::Completed(_));
        if resolved {
            guard.counters.records_ingested += 1;
            if let Some(target) = target {
                guard.pending_resolutions.retain(|peer| *peer != target);
            }
        }
        guard.resolving = None;
        Ok(DiscoveryOutcome {
            resolved,
            ..DiscoveryOutcome::default()
        })
    }

    /// Plan 513: ingests one exploratory `DatabaseSearchReply`.
    ///
    /// The reply's named peers are merged into the lookup's bounded
    /// suggestion buffer and offered to the routing table as
    /// **suggestions**. A suggestion is never evidence: the suggesting peer
    /// must itself be indexed with signed evidence, or the table refuses
    /// the suggestion. A reply from a peer the exploration never asked
    /// introduces nothing.
    pub fn ingest_search_reply(
        &self,
        reply: &DatabaseSearchReplyMessage,
    ) -> Result<DiscoveryOutcome, DiscoveryError> {
        let Ok(mut guard) = self.inner.lock() else {
            return Err(DiscoveryError::NoActiveExploration);
        };
        let source = RouterHash::from_hash(reply.from);
        let suggested: Vec<RouterHash> = reply
            .peer_hashes
            .iter()
            .map(|hash| RouterHash::from_hash(*hash))
            .collect();
        guard
            .seam
            .ingest_exploratory_search_reply(reply)
            .map_err(DiscoveryError::Seam)?;
        guard.counters.search_replies_ingested += 1;
        let (accepted, dropped, unknown_peer) = match guard.exploration.as_mut() {
            Some(exploration) => {
                let result = exploration.job.record_reply(&source, &suggested);
                // Plan 489: only a peer the exploration actually asked may
                // introduce new peers. The job already flags an unknown
                // peer; the spine refuses the named peers outright.
                let known = exploration.asked == Some(source);
                if known {
                    (result.accepted, result.dropped, result.unknown_peer)
                } else {
                    (Vec::new(), suggested.len(), true)
                }
            }
            None => (Vec::new(), suggested.len(), true),
        };
        // The exploration asked one peer and got its answer: it is
        // finished. A finished exploration never blocks the next round.
        guard.exploration = None;
        let mut indexed = 0usize;
        let mut refused = 0usize;
        // The peers the table **accepted**. A refused suggestion names a
        // peer the router does not act on: the table's refusal is a
        // judgement about the source, and a judgement with no consequence
        // is a decoration. Only the accepted peers are queued.
        let mut queueable: Vec<RouterHash> = Vec::new();
        for candidate in &accepted {
            guard.counters.suggestions_offered += 1;
            match guard.table.index_suggested(*candidate, &source) {
                Some(TableInsertOutcome::Inserted) | Some(TableInsertOutcome::Upgraded) => {
                    indexed += 1;
                    queueable.push(*candidate);
                }
                Some(TableInsertOutcome::Idempotent) => {
                    // The table already holds the peer, so the router
                    // knows it. Its name may be resolved.
                    queueable.push(*candidate);
                }
                _ => refused += 1,
            }
        }
        guard.counters.suggestions_refused += refused as u64;
        // The accepted named peers are queued for resolution. A named peer
        // is a **name**: the router cannot ask it until a RouterInfo-kind
        // lookup has returned its signed record.
        for candidate in &queueable {
            if guard.pending_resolutions.contains(candidate) {
                continue;
            }
            if guard.pending_resolutions.len() >= MAX_PENDING_RESOLUTIONS {
                break;
            }
            guard.pending_resolutions.push_back(*candidate);
        }
        Ok(DiscoveryOutcome {
            suggested: suggested.len(),
            indexed,
            refused,
            dropped,
            unknown_peer,
            resolved: false,
        })
    }
}

/// Maps a lookup action onto a discovery dispatch.
fn discovery_dispatch_from_action(
    action: LookupAction,
    delivery: DiscoveryDelivery,
) -> DiscoveryDispatch {
    match action {
        LookupAction::SendDatabaselookup {
            lookup_id,
            peer,
            message,
        } => DiscoveryDispatch::Send {
            lookup_id,
            peer,
            message,
            delivery,
        },
        LookupAction::NeedExploratoryReplyPath { lookup_id } => {
            DiscoveryDispatch::NeedsReplyPath { lookup_id }
        }
        LookupAction::Complete { lookup_id, outcome } => DiscoveryDispatch::Terminal {
            lookup_id,
            final_state: outcome.final_state(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version_eligibility::version_receipt_digest;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_netdb::{RouterInfoStoreConfig, ValidationContext, router_hash};
    use i2pr_proto::{Date, Mapping};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate(signer: &RouterIdentityBundle, published_ms: u64) -> ValidatedRouterInfo {
        let info = signer
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(published_ms)),
        )
        .expect("validate")
    }

    fn key_of(signer: &RouterIdentityBundle) -> RouterHash {
        router_hash(signer.identity()).expect("hash")
    }

    #[test]
    fn learning_a_peer_records_a_receipt_without_naming_it() {
        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        let signer = bundle(0xC00);
        driver.advance_time(1);
        let outcome = driver.learn(validate(&signer, 1));
        assert_eq!(outcome, ProfileInsertOutcome::Inserted);
        let receipts = driver.receipts();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].kind, ParticipationReceiptKind::Learned);
        assert_eq!(receipts[0].profile_count, 1);
        let rendered = format!("{:?}", receipts[0]);
        assert!(
            !rendered.contains(&format!("{:?}", key_of(&signer))),
            "a receipt must not name a peer"
        );
    }

    #[test]
    fn a_batch_learn_emits_one_receipt_and_summarises_the_outcomes() {
        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        driver.advance_time(1);
        let signer = bundle(0xC04);
        let summary = driver.learn_all([validate(&signer, 1), validate(&signer, 1)]);
        assert_eq!(summary.inserted, 1);
        assert_eq!(summary.idempotent, 1);
        assert_eq!(driver.receipt_count(), 1);
        assert_eq!(driver.receipts()[0].kind, ParticipationReceiptKind::Learned);
    }

    #[test]
    fn the_driver_adopts_an_existing_store() {
        let mut source = RouterInfoStore::with_config(RouterInfoStoreConfig::default());
        let signer = bundle(0xC05);
        source.insert(validate(&signer, 1));
        let mut driver = PeerParticipationDriver::adopt(ProfileStoreConfig::default(), source, 10);
        driver.advance_time(10);
        assert_eq!(driver.store().len(), 1);
        assert_eq!(driver.stats().profile_count, 1);
    }

    #[test]
    fn the_shared_handle_applies_a_closure_to_the_driver() {
        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        driver.advance_time(1);
        let signer = bundle(0xC06);
        driver.learn(validate(&signer, 1));
        let handle = PeerParticipationHandle::new(driver);
        let count = handle.with(|driver| driver.store().len());
        assert_eq!(count, Some(1));
        let learned = handle.with(|driver| {
            let signer = bundle(0xC07);
            driver.learn(validate(&signer, 1))
        });
        assert_eq!(learned, Some(ProfileInsertOutcome::Inserted));
    }

    #[test]
    fn the_receipt_log_is_bounded() {
        let mut driver = PeerParticipationDriver::with_limits(ProfileStoreConfig::default(), 4);
        let signer = bundle(0xC01);
        driver.advance_time(1);
        for _ in 0..20 {
            driver.learn(validate(&signer, 1));
        }
        assert_eq!(driver.receipt_count(), 4);
    }

    #[test]
    fn the_maintenance_pass_runs_only_when_due() {
        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        driver.advance_time(0);
        assert!(driver.tick(0).is_some());
        assert!(
            driver
                .tick(i2pr_netdb::DEFAULT_MAINTENANCE_INTERVAL_MS - 1)
                .is_none()
        );
        assert!(
            driver
                .tick(i2pr_netdb::DEFAULT_MAINTENANCE_INTERVAL_MS)
                .is_some()
        );
    }

    #[test]
    fn a_corrupt_projection_is_discarded_and_counted() {
        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        let outcome = driver.load_projection(b"not a projection");
        assert_eq!(
            outcome,
            ProjectionLoadOutcome::Discarded {
                reason: "malformed"
            }
        );
        assert_eq!(driver.discarded_projection_count(), 1);
    }

    #[test]
    fn an_oversized_projection_is_discarded_and_counted() {
        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        let oversized = vec![b'x'; 1024 * 1024];
        let outcome = driver.load_projection(&oversized);
        assert_eq!(
            outcome,
            ProjectionLoadOutcome::Discarded {
                reason: "oversized"
            }
        );
        assert_eq!(driver.discarded_projection_count(), 1);
    }

    #[test]
    fn a_projection_round_trips_through_the_driver() {
        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        let signer = bundle(0xC02);
        driver.advance_time(10);
        driver.learn(validate(&signer, 1));
        driver.observe_answered(&key_of(&signer));
        let encoded = driver.encode_projection().expect("encode");

        let mut restarted = PeerParticipationDriver::new(ProfileStoreConfig::default());
        restarted.advance_time(20);
        restarted.learn(validate(&signer, 1));
        let outcome = restarted.load_projection(&encoded);
        assert_eq!(
            outcome,
            ProjectionLoadOutcome::Restored {
                restored: 1,
                orphaned: 0,
                fresh: 0
            }
        );
    }

    #[test]
    fn the_selection_records_participation() {
        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        let signer = bundle(0xC03);
        driver.advance_time(1);
        driver.learn(validate(&signer, 1));
        let first = driver.select(4, 5);
        assert_eq!(first.len(), 1);
        let second = driver.select(4, 5);
        assert_eq!(second.len(), 1);
    }

    #[test]
    fn the_version_receipts_are_unchanged_by_the_move() {
        // The Plan 491 private mesh: five peers publishing five different API
        // versions. The two receipts' digests are pinned to the values the
        // pre-move implementation produced, so a drift in the digest
        // algorithm is caught here and not only in the evidence lane.
        const GOLDEN_TUNNEL_DIGEST: &str =
            "a1c7672f10901a1ff63a0fcad72194e318169b501660fac3c6607dba54b2d73a";
        const GOLDEN_DSM_DIGEST: &str =
            "02a33179621bce2730a80dfa5cd662ba64f2f3d629ad588d313c195a9234bf5c";

        fn mesh_peer(
            signer: &RouterIdentityBundle,
            published_ms: u64,
            version: Option<&str>,
        ) -> ValidatedRouterInfo {
            let mut options = Mapping::builder();
            if let Some(version) = version {
                options
                    .insert("router.version".to_owned(), version.to_owned())
                    .expect("router.version");
            }
            options
                .insert("caps".to_owned(), "f".to_owned())
                .expect("caps");
            let info = signer
                .sign_router_info(
                    Date::from_millis(published_ms),
                    Vec::new(),
                    Vec::new(),
                    options.build().expect("options"),
                )
                .expect("sign");
            ValidatedRouterInfo::from_router_info(
                info,
                None,
                ValidationContext::new(Date::from_millis(published_ms)),
            )
            .expect("validate")
        }

        fn hex(bytes: &[u8]) -> String {
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        }

        let mut driver = PeerParticipationDriver::new(ProfileStoreConfig::default());
        for (seed, version) in [
            (0x4911u64, Some("0.9.62")),
            (0x4912, Some("0.9.70")),
            (0x4913, Some("0.9.61")),
            (0x4914, None),
            (0x4915, Some("0.9.62.")),
        ] {
            driver.learn(mesh_peer(&bundle(seed), 60_000, version));
        }

        let tunnel = driver.select_tunnel_build_peers(8, 0x4910);
        let dsm = driver.select_dsm_targets(8, 0x4920);
        let receipts = driver.version_receipts();
        assert_eq!(receipts.len(), 2);

        let tunnel_receipt = receipts
            .iter()
            .find(|receipt| receipt.kind == VersionDecisionKind::TunnelBuildPeer)
            .expect("tunnel-build receipt");
        assert_eq!(tunnel_receipt.examined, 5);
        assert_eq!(tunnel_receipt.accepted, 2);
        assert_eq!(hex(&tunnel_receipt.digest), GOLDEN_TUNNEL_DIGEST);
        assert_eq!(
            tunnel_receipt.digest,
            version_receipt_digest(VersionDecisionKind::TunnelBuildPeer, 5, 2, &tunnel.refusals,)
        );

        let dsm_receipt = receipts
            .iter()
            .find(|receipt| receipt.kind == VersionDecisionKind::DsmTarget)
            .expect("dsm receipt");
        assert_eq!(dsm_receipt.examined, 5);
        assert_eq!(dsm_receipt.accepted, 2);
        assert_eq!(hex(&dsm_receipt.digest), GOLDEN_DSM_DIGEST);
        assert_eq!(
            dsm_receipt.digest,
            version_receipt_digest(VersionDecisionKind::DsmTarget, 5, 2, &dsm.refusals)
        );
    }

    #[tokio::test]
    async fn the_periodic_owner_stops_on_cancellation() {
        let handle = PeerParticipationHandle::new(PeerParticipationDriver::new(
            ProfileStoreConfig::default(),
        ));
        let cancellation = i2pr_runtime::CancellationToken::new();
        let token = cancellation.clone();
        let cancel_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            token.cancel(i2pr_core::CancellationReason::OperatorRequest);
        });
        let exit = run_profile_maintenance(handle, cancellation, Duration::from_millis(5)).await;
        cancel_task.await.expect("cancel task");
        assert_eq!(exit, ProfileMaintenanceExit::Cancelled);
    }

    #[tokio::test]
    async fn the_periodic_owner_refuses_an_out_of_bound_period() {
        let handle = PeerParticipationHandle::new(PeerParticipationDriver::new(
            ProfileStoreConfig::default(),
        ));
        let exit = run_profile_maintenance(
            handle,
            i2pr_runtime::CancellationToken::new(),
            Duration::ZERO,
        )
        .await;
        assert_eq!(exit, ProfileMaintenanceExit::InvalidPeriod);
    }
}

// ===========================================================================
// Plan 489: the daemon-side floodfill-indexed routing table driver.
// ===========================================================================

use std::fmt;

/// The kind of event a routing receipt describes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingReceiptKind {
    /// A peer entered the routing table.
    Indexed,
    /// A peer was refused entry to the routing table.
    Refused,
    /// An exploratory `DatabaseLookup` was issued.
    Explored,
    /// An exploration reply named peers that entered the table.
    Discovered,
    /// A lookup was issued from the routing table.
    LookedUp,
    /// A lookup terminated in a final state.
    Terminal,
    /// The table rotated onto a new UTC day.
    Rotated,
    /// A persisted projection was loaded or discarded.
    Projection,
}

impl RoutingReceiptKind {
    /// Returns the stable label of the receipt kind.
    pub const fn kind_label(&self) -> &'static str {
        match self {
            Self::Indexed => "indexed",
            Self::Refused => "refused",
            Self::Explored => "explored",
            Self::Discovered => "discovered",
            Self::LookedUp => "looked_up",
            Self::Terminal => "terminal",
            Self::Rotated => "rotated",
            Self::Projection => "projection",
        }
    }
}

/// A sanitized receipt describing one routing-table event.
///
/// The receipt carries aggregate counts and a digest over those counts.
/// It carries no peer identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingReceipt {
    /// The kind of event this receipt describes.
    pub kind: RoutingReceiptKind,
    /// The number of peers the table retains.
    pub table_size: u32,
    /// The number of non-empty buckets.
    pub occupied_buckets: u32,
    /// The number of entries backed by the peer's own signed `f`.
    pub signed_count: u32,
    /// The number of entries a verified floodfill named.
    pub suggested_count: u32,
    /// The number of peers the router may ask directly.
    pub queryable_count: u32,
    /// The number of peers whose `RouterInfo` must be resolved first.
    pub pending_count: u32,
    /// The number of peers refused because they are not floodfills.
    pub refused_count: u32,
    /// The number of peers the exploration queried.
    pub explored_count: u32,
    /// The number of lookups that terminated in each final state, keyed
    /// by the stable state label.
    ///
    /// The counts are cumulative over the driver's lifetime. A lookup
    /// that spent its peer budget is counted under
    /// `peer-budget-exhausted`, which is a different bucket from the
    /// `peer-exhausted` a candidate-list exhaustion lands in.
    pub terminal_counts: BTreeMap<&'static str, u32>,
    /// A digest over the receipt's counts.
    pub digest: [u8; 32],
}

/// The bounded configuration of the routing table driver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutingTableConfig {
    /// The bounded capacities of the table.
    pub table: BucketSetConfig,
    /// The bounded policy of one exploration.
    pub exploration: ExplorationPolicy,
    /// The maximum number of queryable candidates one lookup carries.
    pub max_queryable: usize,
    /// The maximum number of pending candidates one lookup carries.
    pub max_pending: usize,
}

impl Default for RoutingTableConfig {
    fn default() -> Self {
        Self {
            table: BucketSetConfig::default(),
            exploration: ExplorationPolicy::default(),
            max_queryable: i2pr_netdb::MAX_OPTIMIZED_QUERYABLE,
            max_pending: i2pr_netdb::MAX_OPTIMIZED_PENDING,
        }
    }
}

/// The outcome of starting a table-driven request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingDispatch {
    /// A `DatabaseLookup` must be dispatched to the named peer.
    Send {
        /// The lookup identity.
        lookup_id: LookupId,
        /// The peer to ask.
        peer: RouterHash,
        /// The outbound body the runtime must deliver.
        message: DatabaseLookupMessage,
    },
    /// The request needs a reply path before it can be dispatched.
    NeedsReplyPath {
        /// The lookup identity awaiting a reply path.
        lookup_id: LookupId,
    },
    /// The request is terminal.
    Terminal {
        /// The lookup identity.
        lookup_id: LookupId,
        /// The classified final state.
        final_state: LookupFinalState,
    },
}

/// The outcome of starting an optimized lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingLookupOutcome {
    /// A `DatabaseLookup` must be dispatched to the named peer.
    Send {
        /// The lookup identity.
        lookup_id: LookupId,
        /// The peer to ask.
        peer: RouterHash,
        /// The outbound body the runtime must deliver.
        message: DatabaseLookupMessage,
    },
    /// The table knows peers, but none of their `RouterInfo`s are
    /// resolved yet. The caller must resolve one before the lookup can
    /// be dispatched.
    NeedsPeerResolution {
        /// The number of peers awaiting resolution.
        pending: usize,
    },
    /// The request needs a reply path before it can be dispatched.
    NeedsReplyPath {
        /// The lookup identity awaiting a reply path.
        lookup_id: LookupId,
    },
    /// The request is terminal.
    Terminal {
        /// The lookup identity.
        lookup_id: LookupId,
        /// The classified final state.
        final_state: LookupFinalState,
    },
}

/// The outcome of ingesting an exploration reply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingDiscovery {
    /// The number of peers the reply named.
    pub suggested: usize,
    /// The number of named peers that entered the table.
    pub indexed: usize,
    /// The number of named peers the table refused.
    pub refused: usize,
    /// The number of suggestions dropped by the exploration's ceiling.
    pub dropped: usize,
    /// `true` when the reply named a peer the exploration never asked.
    pub unknown_peer: bool,
}

/// The outcome of loading a persisted routing-table projection.
///
/// A projection that is corrupt, oversized or expired is **discarded
/// and counted**. It never fails the router's startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingProjectionOutcome {
    /// The projection was applied.
    Restored {
        /// The number of peers the projection restored.
        peers: usize,
    },
    /// The projection was discarded.
    Discarded {
        /// A bounded, human-readable reason.
        reason: &'static str,
    },
}

/// The daemon's owner of the floodfill-indexed routing table.
pub struct RoutingTableDriver {
    table: BucketSet,
    lookup: RouterInfoLookup,
    exploration: RouterInfoLookup,
    exploration_job: Option<ExplorationJob>,
    exploration_policy: ExplorationPolicy,
    policy: LookupPolicy,
    config: RoutingTableConfig,
    receipts: VecDeque<RoutingReceipt>,
    max_receipts: usize,
    terminal_counts: BTreeMap<&'static str, u32>,
    clock_ms: u64,
    request_counter: u64,
    active_routing_key: RouterHash,
    reply_path: Option<ReplyPath>,
    discarded_projections: u32,
    last_queryable: usize,
    last_pending: usize,
    last_refused: usize,
}

impl fmt::Debug for RoutingTableDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RoutingTableDriver")
            .field("table_size", &self.table.len())
            .field("clock_ms", &self.clock_ms)
            .field("receipt_count", &self.receipts.len())
            .field("exploration_active", &self.exploration_job.is_some())
            .finish_non_exhaustive()
    }
}

impl RoutingTableDriver {
    /// Constructs a driver anchored at the local router's position in
    /// today's keyspace.
    ///
    /// The anchor is `daily_routing_key(local_hash, date)`: the
    /// routing table partitions the floodfill set around the router's
    /// own position, and the partition rotates with the UTC day.
    pub fn new(local_hash: RouterHash, now: Date, config: RoutingTableConfig) -> Self {
        let anchor = anchor_for(local_hash, now);
        Self {
            table: BucketSet::new(anchor, now, config.table),
            lookup: RouterInfoLookup::new(LookupPolicy::default()),
            exploration: RouterInfoLookup::new(LookupPolicy::default()),
            exploration_job: None,
            exploration_policy: config.exploration,
            policy: LookupPolicy::default(),
            config,
            receipts: VecDeque::new(),
            max_receipts: i2pr_netdb::MAX_OPTIMIZED_QUERYABLE.max(1),
            terminal_counts: BTreeMap::new(),
            clock_ms: 0,
            request_counter: 0,
            active_routing_key: anchor,
            reply_path: None,
            discarded_projections: 0,
            last_queryable: 0,
            last_pending: 0,
            last_refused: 0,
        }
    }

    /// Constructs a driver with an explicit receipt cap.
    pub fn with_limits(
        local_hash: RouterHash,
        now: Date,
        config: RoutingTableConfig,
        max_receipts: usize,
    ) -> Self {
        let mut driver = Self::new(local_hash, now, config);
        driver.max_receipts = max_receipts;
        driver
    }

    /// Returns the routing table.
    pub const fn table(&self) -> &BucketSet {
        &self.table
    }

    /// Returns the driver's current clock reading.
    pub const fn clock_ms(&self) -> u64 {
        self.clock_ms
    }

    /// Advances the driver's clock.
    pub fn advance_time(&mut self, now_ms: u64) {
        self.clock_ms = now_ms;
    }

    /// Sets the reply path the driver attaches to the requests it
    /// issues.
    ///
    /// Without a reply path the lookup state machine refuses to emit a
    /// `DatabaseLookup`: a direct peer link is not a reply path.
    pub fn set_reply_path(&mut self, path: Option<ReplyPath>) {
        self.reply_path = path;
    }

    /// Returns the bounded receipt log.
    pub fn receipts(&self) -> Vec<RoutingReceipt> {
        self.receipts.iter().cloned().collect()
    }

    /// Returns the number of receipts the driver retains.
    pub fn receipt_count(&self) -> usize {
        self.receipts.len()
    }

    /// Returns the number of projections the driver discarded.
    pub const fn discarded_projection_count(&self) -> u32 {
        self.discarded_projections
    }

    /// Returns the active exploration's report, if one is running.
    pub fn exploration_report(&self) -> Option<ExplorationReport> {
        self.exploration_job.as_ref().map(ExplorationJob::report)
    }

    /// Returns why the active exploration stopped, if it has.
    pub fn exploration_stop_reason(&self) -> Option<ExplorationStop> {
        self.exploration_job
            .as_ref()
            .and_then(|job| job.stop_reason(self.clock_ms))
    }

    /// Indexes a peer the router learned about.
    ///
    /// A peer whose own signed `RouterInfo` does not advertise `f` is
    /// refused and counted: the table indexes floodfills only.
    pub fn index(&mut self, validated: &ValidatedRouterInfo) -> TableInsertOutcome {
        let outcome = self.table.insert_signed(validated, self.current_date());
        match outcome {
            TableInsertOutcome::Inserted | TableInsertOutcome::Upgraded => {
                self.emit_receipt(RoutingReceiptKind::Indexed, 0)
            }
            TableInsertOutcome::Idempotent => {}
            _ => self.emit_receipt(RoutingReceiptKind::Refused, 0),
        }
        outcome
    }

    /// Indexes a peer a verified floodfill named.
    ///
    /// The suggesting peer must itself be indexed with signed evidence;
    /// a peer that is not a verified floodfill can never introduce
    /// another peer into the table.
    pub fn index_suggested(&mut self, key: RouterHash, from: &RouterHash) -> TableInsertOutcome {
        let outcome = self.table.insert_suggested(key, from, self.current_date());
        match outcome {
            TableInsertOutcome::Inserted | TableInsertOutcome::Upgraded => {
                self.emit_receipt(RoutingReceiptKind::Indexed, 0)
            }
            TableInsertOutcome::Idempotent => {}
            _ => self.emit_receipt(RoutingReceiptKind::Refused, 0),
        }
        outcome
    }

    /// Rotates the table onto the UTC day containing `now`.
    pub fn rotate(&mut self, now: Date, local_hash: RouterHash) -> RotationReport {
        let report = self.table.rotate(now, anchor_for(local_hash, now));
        self.emit_receipt(RoutingReceiptKind::Rotated, 0);
        report
    }

    /// Starts an exploration for the supplied random search key.
    ///
    /// The request is an exploratory `DatabaseLookup` — wire lookup type
    /// `3` — whose key is the random search key. The candidates come
    /// from the routing table, closest to the search key's routing
    /// position first.
    pub fn begin_exploration(
        &mut self,
        store: &RouterInfoStore,
        search_key: RouterHash,
        routing_key: RouterHash,
    ) -> RoutingDispatch {
        let job = ExplorationJob::begin(
            search_key,
            routing_key,
            self.exploration_policy,
            self.clock_ms,
        );
        self.exploration_job = Some(job);
        self.active_routing_key = routing_key;
        let candidates = self.exploration_candidates(store, routing_key);
        let lookup_id = self.next_lookup_id(LookupKind::Exploratory, search_key);
        let dispatch = if candidates.is_empty() {
            self.terminal_dispatch(lookup_id, LookupFinalState::NoEligibleCandidates)
        } else {
            let outcome = self
                .exploration
                .start_with_candidates(lookup_id, &candidates);
            let dispatch = resolve_dispatch(
                &mut self.exploration,
                self.reply_path,
                self.active_routing_key,
                outcome,
                lookup_id,
            );
            if let RoutingDispatch::Terminal { final_state, .. } = &dispatch {
                self.record_terminal(*final_state);
            }
            self.record_exploration_issue(&dispatch);
            dispatch
        };
        self.emit_receipt(RoutingReceiptKind::Explored, 0);
        dispatch
    }

    /// Advances the active exploration to its next peer.
    ///
    /// The exploration's peer budget and deadline gate the advance: an
    /// exhausted budget terminates the exploration honestly rather than
    /// asking one more peer.
    pub fn exploration_next(&mut self, store: &RouterInfoStore) -> RoutingDispatch {
        let Some(job) = self.exploration_job.as_ref() else {
            let lookup_id =
                self.next_lookup_id(LookupKind::Exploratory, RouterHash::from_bytes([0; 32]));
            return self.terminal_dispatch(lookup_id, LookupFinalState::Cancelled);
        };
        let lookup_id = match self.exploration.active_lookup() {
            Some(id) => id,
            None => {
                let id = self.next_lookup_id(LookupKind::Exploratory, job.search_key());
                return self.terminal_dispatch(id, LookupFinalState::Cancelled);
            }
        };
        if job.budget_exhausted() {
            if let Some(job) = self.exploration_job.as_mut() {
                job.complete();
            }
            return self.terminal_dispatch(lookup_id, LookupFinalState::PeerExhausted);
        }
        if job.is_expired(self.clock_ms) {
            if let Some(job) = self.exploration_job.as_mut() {
                job.complete();
            }
            return self.terminal_dispatch(lookup_id, LookupFinalState::Timeout);
        }
        let routing_key = self.active_routing_key;
        let outcome = self.exploration.advance_after_reply(store, &routing_key);
        let dispatch = match outcome {
            StartOutcome::PendingAttempt(action) => dispatch_from_action(action),
            StartOutcome::NeedsReplyPath(_) => RoutingDispatch::NeedsReplyPath { lookup_id },
            StartOutcome::Terminal(result) => {
                let final_state = failure_state(&result);
                self.record_terminal(final_state);
                RoutingDispatch::Terminal {
                    lookup_id,
                    final_state,
                }
            }
        };
        self.record_exploration_issue(&dispatch);
        dispatch
    }

    /// Records the peer an exploration just issued against, so the
    /// exploration's budget and its outstanding table stay in step with
    /// the state machine.
    fn record_exploration_issue(&mut self, dispatch: &RoutingDispatch) {
        let RoutingDispatch::Send { peer, .. } = dispatch else {
            return;
        };
        if let Some(job) = self.exploration_job.as_mut() {
            job.record_issued(peer);
        }
    }

    /// Returns the peers an exploration may ask, closest first.
    ///
    /// Only peers whose `RouterInfo` the router holds may be asked: a
    /// peer without an address cannot be reached.
    fn exploration_candidates(
        &self,
        store: &RouterInfoStore,
        routing_key: RouterHash,
    ) -> Vec<RouterHash> {
        self.plan(store, routing_key, &[]).queryable_hashes()
    }

    /// Ingests an exploration reply.
    ///
    /// The reply's peers are merged into the engine's suggestion buffer
    /// and offered to the routing table. Only a peer the exploration
    /// actually asked may introduce new peers, and only a verified
    /// floodfill's suggestions are accepted.
    pub fn ingest_search_reply(
        &mut self,
        lookup_id: LookupId,
        reply: &DatabaseSearchReplyMessage,
    ) -> Result<RoutingDiscovery, LookupEngineError> {
        let source = RouterHash::from_hash(reply.from);
        let suggested: Vec<RouterHash> = reply
            .peer_hashes
            .iter()
            .map(|hash| RouterHash::from_hash(*hash))
            .collect();
        let outcome = i2pr_netdb::handle_search_reply(
            &mut self.exploration,
            lookup_id,
            reply,
            self.policy.max_suggested_hashes(),
            self.policy.suggested_hash_limit(),
        )?;
        let _ = outcome;

        let (accepted, dropped, unknown_peer) = match self.exploration_job.as_mut() {
            Some(job) => {
                let result = job.record_reply(&source, &suggested);
                (result.accepted, result.dropped, result.unknown_peer)
            }
            None => (Vec::new(), suggested.len(), true),
        };
        let mut indexed = 0usize;
        let mut refused = 0usize;
        for candidate in &accepted {
            match self
                .table
                .insert_suggested(*candidate, &source, self.current_date())
            {
                TableInsertOutcome::Inserted | TableInsertOutcome::Upgraded => indexed += 1,
                TableInsertOutcome::Idempotent => {}
                _ => refused += 1,
            }
        }
        self.emit_receipt(RoutingReceiptKind::Discovered, 0);
        Ok(RoutingDiscovery {
            suggested: suggested.len(),
            indexed,
            refused,
            dropped,
            unknown_peer,
        })
    }

    /// Starts an optimized lookup for the supplied target.
    ///
    /// The candidates come from the routing table. A peer whose
    /// `RouterInfo` the router has resolved is *queryable*; a peer the
    /// table knows but the router has not resolved is *pending*, and the
    /// lookup refuses to start until one of them is resolved — a peer
    /// without an address cannot be asked.
    pub fn begin_lookup(
        &mut self,
        store: &RouterInfoStore,
        target: RouterHash,
        routing_key: RouterHash,
    ) -> RoutingLookupOutcome {
        let plan = self.plan(store, routing_key, &[]);
        self.last_queryable = plan.queryable.len();
        self.last_pending = plan.pending.len();
        self.last_refused = plan.refused;
        self.active_routing_key = routing_key;
        if plan.queryable.is_empty() {
            if plan.pending.is_empty() {
                let lookup_id = self.next_lookup_id(LookupKind::RouterInfo, target);
                self.record_terminal(LookupFinalState::NoEligibleCandidates);
                return RoutingLookupOutcome::Terminal {
                    lookup_id,
                    final_state: LookupFinalState::NoEligibleCandidates,
                };
            }
            return RoutingLookupOutcome::NeedsPeerResolution {
                pending: plan.pending.len(),
            };
        }
        let lookup_id = self.next_lookup_id(LookupKind::RouterInfo, target);
        let candidates = plan.queryable_hashes();
        let outcome = self.lookup.start_with_candidates(lookup_id, &candidates);
        self.emit_receipt(RoutingReceiptKind::LookedUp, 0);
        match resolve_dispatch(
            &mut self.lookup,
            self.reply_path,
            self.active_routing_key,
            outcome,
            lookup_id,
        ) {
            RoutingDispatch::Send {
                lookup_id,
                peer,
                message,
            } => RoutingLookupOutcome::Send {
                lookup_id,
                peer,
                message,
            },
            RoutingDispatch::NeedsReplyPath { lookup_id } => {
                RoutingLookupOutcome::NeedsReplyPath { lookup_id }
            }
            RoutingDispatch::Terminal {
                lookup_id,
                final_state,
            } => {
                self.record_terminal(final_state);
                RoutingLookupOutcome::Terminal {
                    lookup_id,
                    final_state,
                }
            }
        }
    }

    /// Plan 513: starts the `LookupKind::RouterInfo` lookup that resolves
    /// a named peer the table holds but the router has not resolved.
    ///
    /// This is the **peer-resolution path**. [`Self::begin_lookup`] refuses
    /// to start while the table's peers are all *pending*: their records
    /// are exactly the ones the router is missing, so asking the table for
    /// the candidates to resolve them would never terminate. The
    /// candidates for a resolution therefore come from the
    /// **authoritative flat store** — the router asks the floodfills it
    /// already knows for the named peer's record.
    ///
    /// The reply travels the inbound exploratory tunnel the driver was
    /// given through [`Self::set_reply_path`]; the request travels the
    /// outbound exploratory tunnel the caller composes it through.
    pub fn begin_peer_resolution(
        &mut self,
        store: &RouterInfoStore,
        target: RouterHash,
        routing_key: RouterHash,
    ) -> RoutingLookupOutcome {
        self.active_routing_key = routing_key;
        let lookup_id = self.next_lookup_id(LookupKind::RouterInfo, target);
        let outcome = self.lookup.start(store, lookup_id, &routing_key);
        self.emit_receipt(RoutingReceiptKind::LookedUp, 0);
        match resolve_dispatch(
            &mut self.lookup,
            self.reply_path,
            self.active_routing_key,
            outcome,
            lookup_id,
        ) {
            RoutingDispatch::Send {
                lookup_id,
                peer,
                message,
            } => RoutingLookupOutcome::Send {
                lookup_id,
                peer,
                message,
            },
            RoutingDispatch::NeedsReplyPath { lookup_id } => {
                RoutingLookupOutcome::NeedsReplyPath { lookup_id }
            }
            RoutingDispatch::Terminal {
                lookup_id,
                final_state,
            } => {
                self.record_terminal(final_state);
                RoutingLookupOutcome::Terminal {
                    lookup_id,
                    final_state,
                }
            }
        }
    }

    /// Returns the optimized lookup plan for the supplied routing key.
    pub fn plan(
        &self,
        store: &RouterInfoStore,
        routing_key: RouterHash,
        excluded: &[RouterHash],
    ) -> OptimizedPlan {
        plan_optimized_lookup(
            &self.table,
            store,
            &routing_key,
            excluded,
            self.config.max_queryable,
            self.config.max_pending,
        )
    }

    /// Returns the number of peers the last plan found queryable.
    pub const fn last_queryable_count(&self) -> usize {
        self.last_queryable
    }

    /// Returns the number of peers the last plan found pending.
    pub const fn last_pending_count(&self) -> usize {
        self.last_pending
    }

    /// Returns the number of peers the last plan refused.
    pub const fn last_refused_count(&self) -> usize {
        self.last_refused
    }

    /// Ingests the answer to an optimized lookup.
    ///
    /// A completed answer reaches the caller as
    /// [`ResponseOutcome::Completed`]; the caller holds the validated
    /// record.
    pub fn ingest_store(
        &mut self,
        store: &mut RouterInfoStore,
        lookup_id: LookupId,
        message: &DatabaseStoreMessage,
        context: ValidationContext,
    ) -> Result<ResponseOutcome, LookupEngineError> {
        i2pr_netdb::handle_database_store(&mut self.lookup, store, lookup_id, message, context)
    }

    /// Cancels the active exploration, releasing its outstanding slots.
    pub fn cancel_exploration(&mut self) {
        if let Some(job) = self.exploration_job.as_mut() {
            job.cancel();
        }
    }

    /// Encodes the bounded, sanitized projection of the routing table.
    pub fn encode_projection(&self) -> Vec<u8> {
        self.table.encode_projection()
    }

    /// Loads a persisted projection.
    ///
    /// A corrupt, oversized or **expired** projection is discarded and
    /// counted. A restart never resurrects the previous day's table.
    pub fn load_projection(&mut self, bytes: &[u8], now: Date) -> RoutingProjectionOutcome {
        let outcome = match BucketSet::decode_projection(
            bytes,
            now,
            self.config.table,
            self.table.anchor(),
        ) {
            Ok(table) => {
                let peers = table.len();
                self.table = table;
                RoutingProjectionOutcome::Restored { peers }
            }
            Err(error) => {
                self.discarded_projections = self.discarded_projections.saturating_add(1);
                RoutingProjectionOutcome::Discarded {
                    reason: routing_projection_discard_reason(&error),
                }
            }
        };
        self.emit_receipt(RoutingReceiptKind::Projection, 0);
        outcome
    }

    /// Returns the current date the driver derives from its clock.
    ///
    /// The driver's clock is in milliseconds; the date is the UTC day
    /// that contains it.
    fn current_date(&self) -> Date {
        Date::from_millis(self.clock_ms)
    }

    /// Returns the next lookup identity.
    fn next_lookup_id(&mut self, kind: LookupKind, target: RouterHash) -> LookupId {
        self.request_counter = self.request_counter.saturating_add(1);
        LookupId::new(self.request_counter, kind, target)
    }

    /// Emits a receipt for the current table state.
    fn emit_receipt(&mut self, kind: RoutingReceiptKind, _selected: u32) {
        let (signed, suggested) = self.table.evidence_counts();
        let receipt = RoutingReceipt {
            kind,
            table_size: self.table.len() as u32,
            occupied_buckets: self.table.occupied_buckets() as u32,
            signed_count: signed as u32,
            suggested_count: suggested as u32,
            queryable_count: self.last_queryable as u32,
            pending_count: self.last_pending as u32,
            refused_count: self.last_refused as u32,
            explored_count: self
                .exploration_job
                .as_ref()
                .map_or(0, |job| job.report().peers_queried) as u32,
            terminal_counts: self.terminal_counts.clone(),
            digest: routing_receipt_digest(
                kind,
                self.table.len(),
                self.table.occupied_buckets(),
                signed,
                suggested,
            ),
        };
        if self.max_receipts == 0 {
            return;
        }
        while self.receipts.len() >= self.max_receipts {
            self.receipts.pop_front();
        }
        self.receipts.push_back(receipt);
    }

    /// Records a terminal lookup state under its own stable label.
    ///
    /// Every terminal state the driver maps is counted in the bucket of
    /// its own label, so a lookup that spent the engine's per-lookup
    /// peer budget (`peer-budget-exhausted`) is visible separately from
    /// a lookup whose candidate list ran out (`peer-exhausted`). The
    /// counts are carried by the receipt, which stays counts-only: it
    /// never names a peer, a key, an endpoint or a payload.
    fn record_terminal(&mut self, final_state: LookupFinalState) {
        let counter = self.terminal_counts.entry(final_state.label()).or_insert(0);
        *counter = counter.saturating_add(1);
        self.emit_receipt(RoutingReceiptKind::Terminal, 0);
    }

    /// Records a terminal state and returns the terminal dispatch that
    /// carries it.
    fn terminal_dispatch(
        &mut self,
        lookup_id: LookupId,
        final_state: LookupFinalState,
    ) -> RoutingDispatch {
        self.record_terminal(final_state);
        RoutingDispatch::Terminal {
            lookup_id,
            final_state,
        }
    }
}

/// A placeholder store used when the state machine needs a store
/// reference it will not consult.
///
/// A lookup started with an explicit candidate list never re-selects
/// against the store, so the store the advance path receives is never
/// read for a candidate. The placeholder keeps the existing engine
/// signature — and its fail-closed behaviour — unchanged.
fn empty_router_info_store() -> RouterInfoStore {
    RouterInfoStore::with_config(i2pr_netdb::RouterInfoStoreConfig::new(0, 0))
}

/// Drives a start outcome to a dispatch, attaching the reply path when
/// one is available.
///
/// The function is free-standing so the driver is never borrowed twice:
/// the reply path and the routing key are read out before the state
/// machine is advanced.
fn resolve_dispatch(
    engine: &mut RouterInfoLookup,
    reply_path: Option<ReplyPath>,
    routing_key: RouterHash,
    outcome: StartOutcome,
    lookup_id: LookupId,
) -> RoutingDispatch {
    match outcome {
        StartOutcome::PendingAttempt(action) => dispatch_from_action(action),
        StartOutcome::NeedsReplyPath(_) => {
            let Some(path) = reply_path else {
                return RoutingDispatch::NeedsReplyPath { lookup_id };
            };
            if !engine.accept_reply_path(lookup_id, path) {
                return RoutingDispatch::NeedsReplyPath { lookup_id };
            }
            let outcome =
                engine.handle_pending_after_path(&empty_router_info_store(), &routing_key);
            match outcome {
                StartOutcome::PendingAttempt(action) => dispatch_from_action(action),
                StartOutcome::NeedsReplyPath(_) => RoutingDispatch::NeedsReplyPath { lookup_id },
                StartOutcome::Terminal(result) => RoutingDispatch::Terminal {
                    lookup_id,
                    final_state: failure_state(&result),
                },
            }
        }
        StartOutcome::Terminal(result) => RoutingDispatch::Terminal {
            lookup_id,
            final_state: failure_state(&result),
        },
    }
}

/// Converts a lookup action into a dispatch.
fn dispatch_from_action(action: LookupAction) -> RoutingDispatch {
    match action {
        LookupAction::SendDatabaselookup {
            lookup_id,
            peer,
            message,
        } => RoutingDispatch::Send {
            lookup_id,
            peer,
            message,
        },
        LookupAction::NeedExploratoryReplyPath { lookup_id } => {
            RoutingDispatch::NeedsReplyPath { lookup_id }
        }
        LookupAction::Complete { lookup_id, outcome } => RoutingDispatch::Terminal {
            lookup_id,
            final_state: outcome.final_state(),
        },
    }
}

/// Returns the classified final state of a terminal lookup result.
fn failure_state(result: &i2pr_netdb::LookupResult) -> LookupFinalState {
    match result {
        i2pr_netdb::LookupResult::Failure { final_state, .. } => *final_state,
        _ => LookupFinalState::Success,
    }
}

/// Returns the anchor position for the supplied local identity.
fn anchor_for(local_hash: RouterHash, now: Date) -> RouterHash {
    match daily_routing_key(&local_hash, now) {
        Ok(hash) => RouterHash::from_hash(hash),
        Err(_) => local_hash,
    }
}

/// Returns a bounded, human-readable reason for a discarded projection.
fn routing_projection_discard_reason(error: &RoutingTableProjectionError) -> &'static str {
    match error {
        RoutingTableProjectionError::TooLarge { .. } => "too_large",
        RoutingTableProjectionError::Malformed { .. } => "malformed",
        RoutingTableProjectionError::UnsupportedVersion(_) => "unsupported_version",
        RoutingTableProjectionError::DigestMismatch => "digest_mismatch",
        RoutingTableProjectionError::TooManyRecords { .. } => "too_many_records",
        RoutingTableProjectionError::UnknownEvidence(_) => "unknown_evidence",
        RoutingTableProjectionError::Config(_) => "invalid_config",
    }
}

/// Returns the digest over a receipt's counts.
fn routing_receipt_digest(
    kind: RoutingReceiptKind,
    table_size: usize,
    occupied_buckets: usize,
    signed: usize,
    suggested: usize,
) -> [u8; 32] {
    let mut body = Vec::with_capacity(64);
    body.extend_from_slice(kind.kind_label().as_bytes());
    body.extend_from_slice(&table_size.to_be_bytes());
    body.extend_from_slice(&occupied_buckets.to_be_bytes());
    body.extend_from_slice(&signed.to_be_bytes());
    body.extend_from_slice(&suggested.to_be_bytes());
    let digest = Hash::digest(&body);
    *digest.as_bytes()
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_netdb::{
        LookupKind, RouterInfoStoreConfig, ValidationContext, daily_routing_key, router_hash,
    };
    use i2pr_proto::Mapping;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    const T0: u64 = 1_000;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validated(signer: &RouterIdentityBundle, caps: &str) -> ValidatedRouterInfo {
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), caps.to_owned()).unwrap();
        let published = Date::from_millis(T0);
        let info = signer
            .sign_router_info(published, Vec::new(), Vec::new(), options.build().unwrap())
            .expect("sign");
        ValidatedRouterInfo::from_router_info(info, None, ValidationContext::new(published))
            .expect("validate")
    }

    fn floodfill(seed: u64) -> ValidatedRouterInfo {
        validated(&bundle(seed), "Nf")
    }

    fn key_of(signer: &RouterIdentityBundle) -> RouterHash {
        router_hash(signer.identity()).expect("hash")
    }

    fn store() -> RouterInfoStore {
        RouterInfoStore::with_config(RouterInfoStoreConfig::new(64, 1 << 20))
    }

    fn new_driver(local: &RouterIdentityBundle) -> RoutingTableDriver {
        let mut driver = RoutingTableDriver::new(
            key_of(local),
            Date::from_millis(T0),
            RoutingTableConfig::default(),
        );
        driver.advance_time(T0);
        driver.set_reply_path(Some(
            ReplyPath::new(RouterHash::from_bytes([9; 32]), 7).expect("path"),
        ));
        driver
    }

    fn routing_key_for(hash: &RouterHash) -> RouterHash {
        let key = daily_routing_key(hash, Date::from_millis(T0)).expect("routing key");
        RouterHash::from_hash(key)
    }

    #[test]
    fn a_non_floodfill_is_refused_by_the_table() {
        let local = bundle(0x4892_0001);
        let mut driver = new_driver(&local);
        let plain = validated(&bundle(0x4892_0002), "N");
        let outcome = driver.index(&plain);
        assert_eq!(outcome, TableInsertOutcome::RejectedNotFloodfill);
        assert_eq!(driver.table().len(), 0);
        let receipt = driver.receipts().last().expect("a receipt").clone();
        assert_eq!(receipt.kind, RoutingReceiptKind::Refused);
        assert_eq!(receipt.table_size, 0);
    }

    #[test]
    fn a_floodfill_enters_the_table_with_signed_evidence() {
        let local = bundle(0x4892_0003);
        let mut driver = new_driver(&local);
        let peer = floodfill(0x4892_0004);
        assert_eq!(driver.index(&peer), TableInsertOutcome::Inserted);
        assert_eq!(driver.table().len(), 1);
        let (signed, suggested) = driver.table().evidence_counts();
        assert_eq!(signed, 1);
        assert_eq!(suggested, 0);
    }

    #[test]
    fn the_exploration_request_is_an_exploratory_lookup() {
        let local = bundle(0x4892_0005);
        let mut driver = new_driver(&local);
        let peer = floodfill(0x4892_0006);
        driver.index(&peer);
        let mut peers = store();
        peers.insert(peer.clone());

        let search_key = RouterHash::from_bytes([42; 32]);
        let routing_key = routing_key_for(&search_key);
        let dispatch = driver.begin_exploration(&peers, search_key, routing_key);
        let RoutingDispatch::Send { message, .. } = dispatch else {
            panic!("the exploration must be dispatchable: {dispatch:?}");
        };
        assert_eq!(
            message.lookup_type, 3,
            "an exploratory lookup uses wire lookup type 3"
        );
        assert_eq!(message.key, *search_key.as_hash());
    }

    #[test]
    fn an_exploration_reply_names_peers_that_enter_the_table() {
        let local = bundle(0x4892_0007);
        let mut driver = new_driver(&local);
        let peer = floodfill(0x4892_0008);
        driver.index(&peer);
        let mut peers = store();
        peers.insert(peer.clone());

        let search_key = RouterHash::from_bytes([43; 32]);
        let routing_key = routing_key_for(&search_key);
        let dispatch = driver.begin_exploration(&peers, search_key, routing_key);
        let RoutingDispatch::Send { lookup_id, .. } = dispatch else {
            panic!("the exploration must be dispatchable: {dispatch:?}");
        };

        let discovered = floodfill(0x4892_0009);
        let reply = DatabaseSearchReplyMessage {
            key: *search_key.as_hash(),
            peer_hashes: vec![*discovered.key().as_hash()],
            from: *peer.key().as_hash(),
        };
        let discovery = driver
            .ingest_search_reply(lookup_id, &reply)
            .expect("the reply is ingested");
        assert_eq!(discovery.suggested, 1);
        assert_eq!(discovery.indexed, 1);
        assert!(!discovery.unknown_peer);
        assert_eq!(driver.table().len(), 2);
        let (signed, suggested) = driver.table().evidence_counts();
        assert_eq!(signed, 1);
        assert_eq!(suggested, 1);
    }

    #[test]
    fn an_unverified_source_cannot_introduce_a_peer() {
        let local = bundle(0x4892_000a);
        let mut driver = new_driver(&local);
        let stranger = floodfill(0x4892_000b);
        let outcome = driver.index_suggested(stranger.key(), &stranger.key());
        assert_eq!(outcome, TableInsertOutcome::RejectedUnverifiedSource);
        assert_eq!(driver.table().len(), 0);
    }

    #[test]
    fn the_optimized_lookup_asks_a_table_member() {
        let local = bundle(0x4892_000c);
        let mut driver = new_driver(&local);
        let alpha = floodfill(0x4892_000d);
        let zeta = floodfill(0x4892_000e);
        // Only alpha is in the routing table. Zeta is in the flat store
        // only.
        driver.index(&alpha);
        let mut peers = store();
        peers.insert(alpha.clone());
        peers.insert(zeta.clone());

        let target = RouterHash::from_bytes([77; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = driver.begin_lookup(&peers, target, routing_key);
        let RoutingLookupOutcome::Send { peer, .. } = outcome else {
            panic!("the lookup must be dispatchable: {outcome:?}");
        };
        assert_eq!(
            peer,
            alpha.key(),
            "the candidate must come from the routing table, not the flat store"
        );
    }

    #[test]
    fn a_lookup_without_a_resolved_peer_needs_resolution() {
        let local = bundle(0x4892_000f);
        let mut driver = new_driver(&local);
        let alpha = floodfill(0x4892_0010);
        driver.index(&alpha);
        // The flat store does not hold alpha, so the table member is
        // pending, not queryable.
        let peers = store();
        let target = RouterHash::from_bytes([78; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = driver.begin_lookup(&peers, target, routing_key);
        assert_eq!(
            outcome,
            RoutingLookupOutcome::NeedsPeerResolution { pending: 1 }
        );
    }

    #[test]
    fn an_empty_table_leaves_the_lookup_without_candidates() {
        let local = bundle(0x4892_0011);
        let mut driver = new_driver(&local);
        let peers = store();
        let target = RouterHash::from_bytes([79; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = driver.begin_lookup(&peers, target, routing_key);
        let RoutingLookupOutcome::Terminal { final_state, .. } = outcome else {
            panic!("the lookup must be terminal: {outcome:?}");
        };
        assert_eq!(final_state, LookupFinalState::NoEligibleCandidates);
    }

    #[test]
    fn the_receipts_never_name_a_peer() {
        let local = bundle(0x4892_0012);
        let mut driver = new_driver(&local);
        let alpha = floodfill(0x4892_0013);
        let plain = validated(&bundle(0x4892_0014), "N");
        driver.index(&alpha);
        driver.index(&plain);
        let rendered = format!("{:?}", driver.receipts());
        for peer in [alpha.key(), plain.key()] {
            let hex: String = peer
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert!(
                !rendered.contains(&hex),
                "a receipt must not carry a peer hash"
            );
        }
    }

    #[test]
    fn the_projection_round_trips_and_a_corrupt_one_is_discarded() {
        let local = bundle(0x4892_0015);
        let mut driver = new_driver(&local);
        let alpha = floodfill(0x4892_0016);
        driver.index(&alpha);
        let encoded = driver.encode_projection();

        let mut restarted = new_driver(&local);
        let outcome = restarted.load_projection(&encoded, Date::from_millis(T0));
        assert_eq!(outcome, RoutingProjectionOutcome::Restored { peers: 1 });
        assert_eq!(restarted.table().len(), 1);

        let outcome = restarted.load_projection(b"corrupt projection", Date::from_millis(T0));
        assert_eq!(
            outcome,
            RoutingProjectionOutcome::Discarded {
                reason: "malformed"
            }
        );
        assert_eq!(restarted.discarded_projection_count(), 1);
    }

    #[test]
    fn an_expired_projection_is_not_resurrected() {
        let local = bundle(0x4892_0017);
        let mut driver = new_driver(&local);
        driver.index(&floodfill(0x4892_0018));
        let encoded = driver.encode_projection();

        let mut restarted = new_driver(&local);
        let later = Date::from_millis(T0 + 86_400_000);
        let outcome = restarted.load_projection(&encoded, later);
        assert_eq!(
            outcome,
            RoutingProjectionOutcome::Restored { peers: 1 },
            "the projection carries its own day, so it decodes"
        );
        assert!(
            restarted.table().is_expired(later),
            "a table built for an earlier day is expired"
        );
    }

    #[test]
    fn the_table_rotates_onto_a_new_day() {
        let local = bundle(0x4892_0019);
        let mut driver = new_driver(&local);
        let alpha = floodfill(0x4892_001a);
        driver.index(&alpha);
        let later = Date::from_millis(T0 + 86_400_000);
        let report = driver.rotate(later, key_of(&local));
        assert!(report.rotated);
        assert_eq!(report.retained, 1);
        assert_eq!(driver.table().len(), 1);
        assert!(!driver.table().is_expired(later));
    }

    #[test]
    fn the_exploration_budget_ends_the_job() {
        let local = bundle(0x4892_001b);
        let mut driver = RoutingTableDriver::new(
            key_of(&local),
            Date::from_millis(T0),
            RoutingTableConfig {
                exploration: ExplorationPolicy {
                    budget: i2pr_netdb::ExplorationBudget {
                        max_peers: 1,
                        deadline_ms: 10_000,
                    },
                    max_suggestions: 8,
                },
                ..RoutingTableConfig::default()
            },
        );
        driver.advance_time(T0);
        driver.set_reply_path(Some(
            ReplyPath::new(RouterHash::from_bytes([9; 32]), 7).expect("path"),
        ));
        let peer = floodfill(0x4892_001c);
        driver.index(&peer);
        let mut peers = store();
        peers.insert(peer.clone());

        let search_key = RouterHash::from_bytes([44; 32]);
        let routing_key = routing_key_for(&search_key);
        let dispatch = driver.begin_exploration(&peers, search_key, routing_key);
        assert!(matches!(dispatch, RoutingDispatch::Send { .. }));
        let next = driver.exploration_next(&peers);
        let RoutingDispatch::Terminal { final_state, .. } = next else {
            panic!("the budget must end the exploration: {next:?}");
        };
        assert_eq!(final_state, LookupFinalState::PeerExhausted);
        assert_eq!(
            driver.exploration_stop_reason(),
            Some(ExplorationStop::BudgetExhausted)
        );
    }

    #[test]
    fn a_lookup_without_a_reply_path_is_not_dispatchable() {
        let local = bundle(0x4892_001d);
        let mut driver = RoutingTableDriver::new(
            key_of(&local),
            Date::from_millis(T0),
            RoutingTableConfig::default(),
        );
        driver.advance_time(T0);
        let peer = floodfill(0x4892_001e);
        driver.index(&peer);
        let mut peers = store();
        peers.insert(peer.clone());
        let target = RouterHash::from_bytes([80; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = driver.begin_lookup(&peers, target, routing_key);
        let RoutingLookupOutcome::NeedsReplyPath { .. } = outcome else {
            panic!("a lookup without a reply path must not be dispatchable: {outcome:?}");
        };
    }

    /// Builds a `DatabaseStore` carrying the peer's own record, as a
    /// peer would answer a `RouterInfo` lookup.
    fn theta_record() -> DatabaseStoreMessage {
        let peer = floodfill(0x4892_0025);
        let encoded = peer.encoded(16 * 1024).expect("encode");
        let mut compressor =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut compressor, &encoded).expect("compress");
        let compressed = compressor.finish().expect("finish");
        let payload = i2pr_proto::DeferredPayload::new(compressed, 16 * 1024).expect("payload");
        DatabaseStoreMessage {
            key: *peer.key().as_hash(),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: i2pr_proto::DatabaseStoreData::RouterInfoCompressed(payload),
        }
    }

    #[test]
    fn a_restart_does_not_resurrect_an_expired_table() {
        let local = bundle(0x4892_0021);
        let mut driver = new_driver(&local);
        driver.index(&floodfill(0x4892_0022));
        let encoded = driver.encode_projection();

        // A restart on a later day restores the bytes, but the restored
        // table carries the day it was built for and is therefore
        // expired against the current day.
        let mut restarted = new_driver(&local);
        let later = Date::from_millis(T0 + 86_400_000);
        let outcome = restarted.load_projection(&encoded, later);
        assert_eq!(outcome, RoutingProjectionOutcome::Restored { peers: 1 });
        assert!(
            restarted.table().is_expired(later),
            "the restored table must be recognised as expired"
        );
    }

    #[test]
    fn a_reply_from_before_a_restart_does_not_complete_a_lookup() {
        let local = bundle(0x4892_0023);
        let mut driver = new_driver(&local);
        let alpha = floodfill(0x4892_0024);
        driver.index(&alpha);
        let mut peers = store();
        peers.insert(alpha.clone());
        let target = RouterHash::from_bytes([82; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = driver.begin_lookup(&peers, target, routing_key);
        let RoutingLookupOutcome::Send { lookup_id, .. } = outcome else {
            panic!("the lookup must be dispatchable: {outcome:?}");
        };

        // The router restarts. The new process has no active lookup, so
        // the reply that was in flight cannot complete anything.
        let mut restarted = new_driver(&local);
        let mut restarted_peers = store();
        restarted_peers.insert(alpha.clone());
        let encoded = restarted.encode_projection();
        restarted.load_projection(&encoded, Date::from_millis(T0));
        let encoded = theta_record();
        let response = restarted
            .ingest_store(
                &mut restarted_peers,
                lookup_id,
                &encoded,
                ValidationContext::new(Date::from_millis(T0)),
            )
            .expect("the stale reply is refused, not fatal");
        assert_eq!(
            response,
            ResponseOutcome::Ignored,
            "a reply from before the restart must not complete a lookup"
        );
    }

    #[test]
    fn the_lookup_kind_is_the_router_info_kind() {
        let local = bundle(0x4892_001f);
        let mut driver = new_driver(&local);
        let peer = floodfill(0x4892_0020);
        driver.index(&peer);
        let mut peers = store();
        peers.insert(peer.clone());
        let target = RouterHash::from_bytes([81; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = driver.begin_lookup(&peers, target, routing_key);
        let RoutingLookupOutcome::Send {
            lookup_id, message, ..
        } = outcome
        else {
            panic!("the lookup must be dispatchable: {outcome:?}");
        };
        assert_eq!(lookup_id.kind(), LookupKind::RouterInfo);
        assert_eq!(message.lookup_type, 2);
    }

    // Plan 497 — the consumer seam's counted outcomes ---------------

    #[test]
    fn the_consumer_answers_from_the_table_and_counts_it() {
        let local = bundle(0x4970_0001);
        let mut driver = new_driver(&local);
        let alpha = floodfill(0x4970_0002);
        driver.index(&alpha);
        let mut peers = store();
        peers.insert(alpha.clone());
        let consumer = RoutingTableConsumer::new(driver, true);
        let target = RouterHash::from_bytes([82; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = consumer.candidates(&peers, &routing_key, &[]);
        let RoutingTableCandidates::Table(candidates) = outcome else {
            panic!("the table must answer: {outcome:?}");
        };
        assert_eq!(candidates, vec![alpha.key()]);
        let counters = consumer.counters();
        assert_eq!(counters.answered, 1);
        assert_eq!(counters.fell_back(), 0);
    }

    #[test]
    fn a_disabled_consumer_falls_back_and_counts_the_reason() {
        let local = bundle(0x4970_0003);
        let consumer = RoutingTableConsumer::new(new_driver(&local), false);
        let peers = store();
        let target = RouterHash::from_bytes([83; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = consumer.candidates(&peers, &routing_key, &[]);
        assert_eq!(
            outcome,
            RoutingTableCandidates::Fallback(RoutingTableFallback::Disabled)
        );
        let counters = consumer.counters();
        assert_eq!(counters.answered, 0);
        assert_eq!(counters.fell_back_disabled, 1);
        assert_eq!(counters.fell_back_no_candidate, 0);
        assert_eq!(counters.fell_back_unavailable, 0);
    }

    #[test]
    fn a_poisoned_table_falls_back_and_counts_the_reason() {
        // A poisoned table cannot answer. The fallback must still be
        // counted, and under its own reason: a fallback that is not
        // counted is a silent fallback.
        let local = bundle(0x4970_0004);
        let consumer = RoutingTableConsumer::new(new_driver(&local), true);
        let inner = Arc::clone(&consumer.inner);
        let _ = std::panic::catch_unwind(move || {
            let _guard = inner.lock().expect("the table's lock is held");
            panic!("poison the table");
        });
        let peers = store();
        let target = RouterHash::from_bytes([84; 32]);
        let routing_key = routing_key_for(&target);
        let outcome = consumer.candidates(&peers, &routing_key, &[]);
        assert_eq!(
            outcome,
            RoutingTableCandidates::Fallback(RoutingTableFallback::Unavailable)
        );
        let counters = consumer.counters();
        assert_eq!(counters.answered, 0);
        assert_eq!(counters.fell_back_unavailable, 1);
        assert_eq!(counters.fell_back_disabled, 0);
        assert_eq!(counters.fell_back_no_candidate, 0);
        assert_eq!(counters.fell_back(), 1);
    }
}
