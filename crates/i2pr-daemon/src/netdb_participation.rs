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
    Candidate, MaintenanceReport, ObserveOutcome, ParticipationLedger, ProfileInsertOutcome,
    ProfileProjection, ProfileProjectionError, ProfileStore, ProfileStoreConfig, ProfileStoreStats,
    RouterHash, RouterInfoStore, SelectionConstraints, ValidatedRouterInfo, select_candidates,
};
use i2pr_proto::Hash;

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
