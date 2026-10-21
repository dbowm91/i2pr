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

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use i2pr_netdb::{
    BucketSet, BucketSetConfig, Candidate, ExplorationJob, ExplorationPolicy, ExplorationReport,
    ExplorationStop, LookupAction, LookupEngineError, LookupFinalState, LookupId, LookupKind,
    LookupPolicy, MaintenanceReport, ObserveOutcome, OptimizedPlan, ParticipationLedger,
    ProfileInsertOutcome, ProfileProjection, ProfileProjectionError, ProfileStore,
    ProfileStoreConfig, ProfileStoreStats, ReplyPath, ResponseOutcome, RotationReport, RouterHash,
    RouterInfoLookup, RouterInfoStore, RoutingTableProjectionError, SelectionConstraints,
    StartOutcome, TableInsertOutcome, ValidatedRouterInfo, ValidationContext, daily_routing_key,
    plan_optimized_lookup, select_candidates,
};
use i2pr_proto::{
    DatabaseLookupMessage, DatabaseSearchReplyMessage, DatabaseStoreMessage, Date, Hash,
};

/// The maximum number of receipts the driver retains.
pub const DEFAULT_MAX_RECEIPTS: usize = 256;

/// The largest maintenance period the daemon accepts.
pub const MAX_MAINTENANCE_PERIOD: Duration = Duration::from_secs(3600);

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

#[cfg(test)]
mod tests {
    use super::*;
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
            RoutingDispatch::Terminal {
                lookup_id,
                final_state: LookupFinalState::NoEligibleCandidates,
            }
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
            return RoutingDispatch::Terminal {
                lookup_id,
                final_state: LookupFinalState::Cancelled,
            };
        };
        let lookup_id = match self.exploration.active_lookup() {
            Some(id) => id,
            None => {
                let id = self.next_lookup_id(LookupKind::Exploratory, job.search_key());
                return RoutingDispatch::Terminal {
                    lookup_id: id,
                    final_state: LookupFinalState::Cancelled,
                };
            }
        };
        if job.budget_exhausted() {
            if let Some(job) = self.exploration_job.as_mut() {
                job.complete();
            }
            return RoutingDispatch::Terminal {
                lookup_id,
                final_state: LookupFinalState::PeerExhausted,
            };
        }
        if job.is_expired(self.clock_ms) {
            if let Some(job) = self.exploration_job.as_mut() {
                job.complete();
            }
            return RoutingDispatch::Terminal {
                lookup_id,
                final_state: LookupFinalState::Timeout,
            };
        }
        let routing_key = self.active_routing_key;
        let outcome = self.exploration.advance_after_reply(store, &routing_key);
        let dispatch = match outcome {
            StartOutcome::PendingAttempt(action) => dispatch_from_action(action),
            StartOutcome::NeedsReplyPath(_) => RoutingDispatch::NeedsReplyPath { lookup_id },
            StartOutcome::Terminal(result) => RoutingDispatch::Terminal {
                lookup_id,
                final_state: failure_state(&result),
            },
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
            } => RoutingLookupOutcome::Terminal {
                lookup_id,
                final_state,
            },
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
}
