//! Plan 106 daemon bootstrap state machine.
//!
//! The bootstrap state machine owns the bounded startup/readiness
//! pipeline for the `i2pr` daemon. It composes the Plan 103/104/105
//! surfaces (`RouterInfoStore`, `LocalRouterInfoBuilder`,
//! `ReseedSignerTrustSet`) without owning a runtime, sockets, or
//! tunnels. It exposes:
//!
//! - a bounded set of typed [`BootstrapState`] values;
//! - a [`BootstrapPolicy`] for cache-sufficient/reseed-required
//!   thresholds;
//! - a [`Bootstrap`] owner that runs cache + optional reseed through
//!   the Plan 104 composition owner and produces a sanitized
//!   [`BootstrapReport`];
//! - privacy-safe [`BootstrapSnapshot`] diagnostics for the daemon
//!   service graph;
//!
//! The module is deliberately synchronous and runtime-neutral. The
//! daemon composition root composes it with the supervisor; the
//! runtime-owned service is registered without re-running any of the
//! pipeline stages.

#![forbid(unsafe_code)]

use std::fmt;
use std::path::Path;

use i2pr_netdb::{
    LocalRouterInfo, LocalRouterInfoBuilder, ReseedSignatureType, ReseedSignerId,
    ReseedSignerTrustSet, RouterHash, RouterInfoStore, RouterInfoStoreConfig, TrustedSigner,
    ValidationContext, trust_signer_from_certificate,
};
use i2pr_netdb_persist::{
    CacheLoader, CacheLoaderReport, ReseedBundleReport, ReseedIngestLimits, ReseedIngestor,
    ReseedSummary,
};
use i2pr_proto::{Date, Mapping};
use i2pr_storage::cache_seam::ByteCache;
use thiserror::Error;

use crate::config::{Config, ReseedConfig, ReseedSourceConfig};

/// Maximum byte count for a single reseed bundle acquisition. The
/// hard ceiling matches the Plan 104 SU3 single-file cap and stops
/// runaway allocations regardless of operator configuration.
pub const MAX_RESEED_BYTES_HARD: usize = 16 * 1024 * 1024;

/// Bounded bootstrap state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapState {
    /// The NetDB store and cache are empty.
    Empty,
    /// The cached record count meets the policy minimum.
    CacheSufficient,
    /// The cached record count is below the policy minimum.
    ReseedRequired,
    /// The reseed pipeline is currently executing.
    Reseeding,
    /// The bootstrap pipeline finished with a healthy store.
    ReadyForNetworkIntegration,
    /// The bootstrap pipeline finished but the store is degraded.
    DegradedInsufficientPeers,
    /// The bootstrap pipeline failed closed.
    Failed,
}

impl BootstrapState {
    /// Returns `true` when the state machine has reached a terminal
    /// bootstrap state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::ReadyForNetworkIntegration | Self::DegradedInsufficientPeers | Self::Failed
        )
    }
}

impl fmt::Display for BootstrapState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Empty => "empty",
            Self::CacheSufficient => "cache-sufficient",
            Self::ReseedRequired => "reseed-required",
            Self::Reseeding => "reseeding",
            Self::ReadyForNetworkIntegration => "ready-for-network-integration",
            Self::DegradedInsufficientPeers => "degraded-insufficient-peers",
            Self::Failed => "failed",
        };
        formatter.write_str(label)
    }
}

/// Typed bootstrap pipeline failure.
#[derive(Debug, Error)]
pub enum BootstrapError {
    /// The data directory could not be prepared.
    #[error("bootstrap failed to prepare data directory: {0}")]
    PrepareDataDir(String),
    /// The reseed trust set could not be loaded from the
    /// configured signer certificates.
    #[error("bootstrap failed to load reseed trust set: {0}")]
    ReseedTrustSet(String),
    /// The reseed ingestion pipeline failed.
    #[error("bootstrap reseed ingestion failed: {0}")]
    Reseed(String),
    /// The local RouterInfo could not be constructed or validated.
    #[error("bootstrap local router info construction failed: {0}")]
    LocalRouterInfo(String),
}

/// Policy values used by the bootstrap state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootstrapPolicy {
    /// Minimum record count required for `CacheSufficient`.
    pub min_router_infos: usize,
    /// Minimum floodfill advertiser count required for
    /// `ReadyForNetworkIntegration`.
    pub min_floodfill_advertisers: usize,
    /// Whether reseed is allowed to run.
    pub reseed_enabled: bool,
}

impl BootstrapPolicy {
    /// Derives the policy from the validated `Config`.
    pub fn from_config(config: &Config) -> Self {
        Self {
            min_router_infos: config.netdb.min_router_infos,
            min_floodfill_advertisers: config.netdb.min_floodfill_advertisers,
            reseed_enabled: config.reseed.enabled,
        }
    }
}

/// Bounded diagnostic snapshot the daemon service graph emits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapSnapshot {
    /// Current bootstrap state.
    pub state: BootstrapState,
    /// Total record count.
    pub record_count: usize,
    /// Total encoded byte count.
    pub encoded_bytes: usize,
    /// Number of records that advertise the `f` capability.
    pub floodfill_advertisers: usize,
    /// Number of reseed attempts that have been made.
    pub reseed_attempts: usize,
    /// Bounded last-reseed summary, when at least one attempt ran.
    pub last_reseed_summary: Option<ReseedSummary>,
}

impl Default for BootstrapSnapshot {
    fn default() -> Self {
        Self {
            state: BootstrapState::Empty,
            record_count: 0,
            encoded_bytes: 0,
            floodfill_advertisers: 0,
            reseed_attempts: 0,
            last_reseed_summary: None,
        }
    }
}

/// Bounded result of one full bootstrap run.
#[derive(Clone, Debug)]
pub struct BootstrapReport {
    /// Final bootstrap state.
    pub final_state: BootstrapState,
    /// Bounded diagnostics.
    pub snapshot: BootstrapSnapshot,
    /// Sanitized per-file cache loader report, when the cache existed.
    pub cache_report: Option<CacheLoaderReport>,
    /// Last reseed bundle report, when a reseed attempt ran.
    pub reseed_report: Option<ReseedBundleReport>,
    /// Sanitized per-attempt reseed outcomes.
    pub reseed_attempts: Vec<ReseedAttemptSummary>,
}

/// Summary of a single reseed attempt. Persisted in the report so
/// callers can audit what each bounded run did without retaining
/// payloads, keys, or full RouterInfo bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReseedAttemptSummary {
    /// Sequence number (1-based).
    pub attempt: usize,
    /// One-line outcome label.
    pub outcome: &'static str,
    /// Total records observed.
    pub total: usize,
    /// Records accepted and inserted.
    pub accepted: usize,
    /// Records rejected at the filename hash.
    pub rejected_filename: usize,
    /// Records rejected at decode.
    pub rejected_decode: usize,
    /// Records rejected at validation.
    pub rejected_validation: usize,
}

/// The Plan 106 daemon bootstrap owner.
///
/// The owner runs once during startup and produces a bounded
/// [`BootstrapReport`]. It owns no Tokio tasks; the daemon service
/// graph consumes the report and registers a long-lived supervisor
/// service that observes the resulting `RouterInfoStore` and
/// `LocalRouterInfo` snapshots.
pub struct Bootstrap {
    store: RouterInfoStore,
    local: Option<LocalRouterInfo>,
    snapshot: BootstrapSnapshot,
    cache_report: Option<CacheLoaderReport>,
    reseed_report: Option<ReseedBundleReport>,
    reseed_attempts: Vec<ReseedAttemptSummary>,
    last_reseed_summary: Option<ReseedSummary>,
    reseed_config: ReseedConfig,
    reseed_offline_path: Option<std::path::PathBuf>,
}

impl Bootstrap {
    /// Constructs a new bootstrap owner with the supplied store
    /// configuration.
    pub fn new(store_config: RouterInfoStoreConfig, reseed_config: ReseedConfig) -> Self {
        Self {
            store: RouterInfoStore::with_config(store_config),
            local: None,
            snapshot: BootstrapSnapshot::default(),
            cache_report: None,
            reseed_report: None,
            reseed_attempts: Vec::new(),
            last_reseed_summary: None,
            reseed_config,
            reseed_offline_path: None,
        }
    }

    /// Sets the optional offline reseed source path used by the
    /// offline reseed pipeline.
    pub fn with_offline_reseed_path(mut self, path: std::path::PathBuf) -> Self {
        self.reseed_offline_path = Some(path);
        self
    }

    /// Returns the populated store after the bootstrap run.
    pub fn store(&self) -> &RouterInfoStore {
        &self.store
    }

    /// Returns the current local RouterInfo snapshot, if any.
    pub fn local(&self) -> Option<&LocalRouterInfo> {
        self.local.as_ref()
    }

    /// Returns the current snapshot.
    pub fn snapshot(&self) -> &BootstrapSnapshot {
        &self.snapshot
    }

    /// Runs the bounded pipeline:
    ///
    /// 1. revalidates the persistent cache through the Plan 104
    ///    composition owner;
    /// 2. constructs and self-validates the local RouterInfo;
    /// 3. recomputes the bootstrap readiness state;
    /// 4. if the policy demands it, performs at most one bounded
    ///    reseed acquisition from the optional offline source;
    /// 5. persists any new validated remote RouterInfos the cache
    ///    loader did not already cover.
    ///
    /// `now_seconds` is the wall-clock time used for freshness checks
    /// and reseed signature verification. The pipeline does not
    /// read the wall clock itself.
    pub fn run(
        &mut self,
        data_dir: &Path,
        builder: &LocalRouterInfoBuilder<'_>,
        policy: BootstrapPolicy,
        now_seconds: u64,
    ) -> Result<BootstrapReport, BootstrapError> {
        i2pr_storage::IdentityStore::prepare_directory(data_dir)
            .map_err(|error| BootstrapError::PrepareDataDir(error.to_string()))?;
        self.run_cache_loader(data_dir, now_seconds)?;
        self.run_local(builder, now_seconds)?;
        let mut final_state = self.compute_state(policy);
        self.snapshot.state = final_state;
        if matches!(final_state, BootstrapState::ReseedRequired) && policy.reseed_enabled {
            self.snapshot.state = BootstrapState::Reseeding;
            final_state = self.run_offline_reseed(data_dir, policy, now_seconds)?;
        }
        let snapshot = self.snapshot.clone();
        Ok(BootstrapReport {
            final_state,
            snapshot,
            cache_report: self.cache_report.clone(),
            reseed_report: self.reseed_report.clone(),
            reseed_attempts: self.reseed_attempts.clone(),
        })
    }

    fn run_cache_loader(
        &mut self,
        data_dir: &Path,
        now_seconds: u64,
    ) -> Result<(), BootstrapError> {
        let cache = ByteCache::in_data_dir(data_dir);
        let loader = CacheLoader::new(cache);
        let now = Date::from_millis(now_seconds.saturating_mul(1000));
        let context = ValidationContext::new(now);
        let report = loader
            .load_into(&mut self.store, context)
            .map_err(|error| BootstrapError::PrepareDataDir(error.to_string()))?;
        self.refresh_snapshot();
        self.cache_report =
            if report.entries_inspected == 0 && report.invalid == 0 && report.unreadable == 0 {
                None
            } else {
                Some(report)
            };
        Ok(())
    }

    fn run_local(
        &mut self,
        builder: &LocalRouterInfoBuilder<'_>,
        now_seconds: u64,
    ) -> Result<(), BootstrapError> {
        let published = Date::from_millis(now_seconds.saturating_mul(1000));
        let local = builder
            .build(published, Mapping::empty())
            .map_err(|error| BootstrapError::LocalRouterInfo(error.to_string()))?;
        self.local = Some(local);
        Ok(())
    }

    fn compute_state(&mut self, policy: BootstrapPolicy) -> BootstrapState {
        self.refresh_snapshot();
        let stats = self.store.stats();
        if stats.record_count == 0 {
            return BootstrapState::Empty;
        }
        if stats.record_count < policy.min_router_infos {
            return BootstrapState::ReseedRequired;
        }
        if stats.floodfill_advertiser_count >= policy.min_floodfill_advertisers {
            BootstrapState::ReadyForNetworkIntegration
        } else {
            BootstrapState::CacheSufficient
        }
    }

    fn run_offline_reseed(
        &mut self,
        data_dir: &Path,
        policy: BootstrapPolicy,
        now_seconds: u64,
    ) -> Result<BootstrapState, BootstrapError> {
        let path = match self.reseed_offline_path.clone() {
            Some(path) => path,
            None => return Ok(self.compute_state(policy)),
        };
        let bundle = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(self.compute_state(policy)),
        };
        let trust = match build_trust_set(&self.reseed_config.sources) {
            Ok(trust) => trust,
            Err(error) => {
                self.reseed_attempts.push(ReseedAttemptSummary {
                    attempt: self.reseed_attempts.len() + 1,
                    outcome: "trust-set-load-failed",
                    total: 0,
                    accepted: 0,
                    rejected_filename: 0,
                    rejected_decode: 0,
                    rejected_validation: 0,
                });
                self.snapshot.reseed_attempts = self.reseed_attempts.len();
                return Err(error);
            }
        };
        let limits = ReseedIngestLimits {
            max_su3_bytes: self.reseed_config.max_su3_bytes.min(MAX_RESEED_BYTES_HARD),
            ..ReseedIngestLimits::default()
        };
        let now_ms = now_seconds.saturating_mul(1000);
        let context = ValidationContext::new(Date::from_millis(now_ms));
        let cache = ByteCache::in_data_dir(data_dir);
        let loader = CacheLoader::new(cache);
        let ingestor = ReseedIngestor::with_limits(&trust, limits);
        match ingestor.ingest_su3_into(
            &bundle,
            now_seconds,
            context,
            &mut self.store,
            Some(&loader),
        ) {
            Ok(report) => {
                let summary = ReseedSummary::from(&report);
                self.reseed_report = Some(report);
                self.last_reseed_summary = Some(summary.clone());
                self.reseed_attempts.push(ReseedAttemptSummary {
                    attempt: self.reseed_attempts.len() + 1,
                    outcome: "completed",
                    total: summary.total,
                    accepted: summary.accepted,
                    rejected_filename: summary.rejected_filename,
                    rejected_decode: summary.rejected_decode,
                    rejected_validation: summary.rejected_validation,
                });
                self.snapshot.reseed_attempts = self.reseed_attempts.len();
                self.refresh_snapshot();
                Ok(self.compute_state(policy))
            }
            Err(error) => {
                let outcome = match &error {
                    i2pr_netdb_persist::reseed_ingest::ReseedIngestError::UnknownSigner => {
                        "unknown-signer"
                    }
                    i2pr_netdb_persist::reseed_ingest::ReseedIngestError::EmptyResult => {
                        "empty-result"
                    }
                    i2pr_netdb_persist::reseed_ingest::ReseedIngestError::Verification(_) => {
                        "verification-failed"
                    }
                };
                self.reseed_attempts.push(ReseedAttemptSummary {
                    attempt: self.reseed_attempts.len() + 1,
                    outcome,
                    total: 0,
                    accepted: 0,
                    rejected_filename: 0,
                    rejected_decode: 0,
                    rejected_validation: 0,
                });
                self.snapshot.reseed_attempts = self.reseed_attempts.len();
                Ok(self.compute_state(policy))
            }
        }
    }

    fn refresh_snapshot(&mut self) {
        let stats = self.store.stats();
        self.snapshot.record_count = stats.record_count;
        self.snapshot.encoded_bytes = stats.total_encoded_bytes;
        self.snapshot.floodfill_advertisers = stats.floodfill_advertiser_count;
        self.snapshot.last_reseed_summary = self.last_reseed_summary.clone();
    }

    /// Returns a typed snapshot suitable for the daemon service
    /// graph diagnostics.
    pub fn diagnostics(&self) -> BootstrapSnapshot {
        self.snapshot.clone()
    }

    /// Replaces the local RouterInfo snapshot. Used by the daemon
    /// composition root when it rebuilds the local record on schedule.
    pub fn replace_local(&mut self, local: LocalRouterInfo) {
        self.local = Some(local);
    }

    /// Returns the current local RouterHash, if any.
    pub fn local_hash(&self) -> Option<RouterHash> {
        self.local.as_ref().map(|local| local.router_hash())
    }

    /// Convenience accessor used by integration tests for the number
    /// of inserted remote RouterInfos that the Plan 104 cache loader
    /// counted during the last run.
    pub fn cache_inserted(&self) -> usize {
        self.cache_report
            .as_ref()
            .map(|report| report.inserted)
            .unwrap_or_default()
    }
}

/// Loads the Plan 104 signer trust set from the configured
/// certificate paths.
pub fn build_trust_set(
    sources: &[ReseedSourceConfig],
) -> Result<ReseedSignerTrustSet, BootstrapError> {
    let mut trust = ReseedSignerTrustSet::new();
    for source in sources {
        let bytes = std::fs::read(&source.certificate_path)
            .map_err(|error| BootstrapError::ReseedTrustSet(error.to_string()))?;
        let signer_id = ReseedSignerId::new(&source.signer_id)
            .map_err(|error| BootstrapError::ReseedTrustSet(error.to_string()))?;
        let signer: TrustedSigner = trust_signer_from_certificate(
            signer_id,
            bytes,
            ReseedSignatureType::RsaSha512_4096,
            0,
            u64::MAX,
        )
        .map_err(|error| BootstrapError::ReseedTrustSet(error.to_string()))?;
        trust.add(signer);
    }
    Ok(trust)
}

/// Helper that runs the bootstrap pipeline using the supplied
/// offline SU3 bundle and produces a sanitized [`BootstrapReport`].
///
/// The helper exposes a single side-effectful boundary so callers
/// (and integration tests) cannot accidentally compose the cache
/// loader, the local builder, and the reseed ingestor in arbitrary
/// orders.
pub fn bootstrap_with_offline_reseed(
    config: &Config,
    builder: &LocalRouterInfoBuilder<'_>,
    now_seconds: u64,
    offline_reseed_path: Option<std::path::PathBuf>,
) -> Result<BootstrapReport, BootstrapError> {
    let store_config =
        RouterInfoStoreConfig::new(config.netdb.max_records, config.netdb.max_encoded_bytes);
    let mut bootstrap = Bootstrap::new(store_config, config.reseed.clone());
    if let Some(path) = offline_reseed_path {
        bootstrap = bootstrap.with_offline_reseed_path(path);
    }
    let policy = BootstrapPolicy::from_config(config);
    bootstrap.run(&config.router.data_dir, builder, policy, now_seconds)
}

/// Convenience wrapper that constructs a `BootstrapSnapshot` from
/// the in-memory store so callers can quickly inspect state.
pub fn store_summary(store: &RouterInfoStore) -> BootstrapSnapshot {
    let stats = store.stats();
    BootstrapSnapshot {
        state: BootstrapState::Empty,
        record_count: stats.record_count,
        encoded_bytes: stats.total_encoded_bytes,
        floodfill_advertisers: stats.floodfill_advertiser_count,
        reseed_attempts: 0,
        last_reseed_summary: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_store_reports_empty_state() {
        let config = ReseedConfig {
            enabled: false,
            max_sources: 4,
            max_su3_bytes: 1024,
            sources: Vec::new(),
        };
        let bootstrap = Bootstrap::new(RouterInfoStoreConfig::default(), config);
        assert_eq!(bootstrap.snapshot().state, BootstrapState::Empty);
        assert_eq!(bootstrap.snapshot().record_count, 0);
    }

    #[test]
    fn build_trust_set_rejects_missing_certificate() {
        let sources = vec![ReseedSourceConfig {
            signer_id: "missing".to_owned(),
            certificate_path: std::path::PathBuf::from("/nope/cert.pem"),
        }];
        let error = build_trust_set(&sources).unwrap_err();
        assert!(matches!(error, BootstrapError::ReseedTrustSet(_)));
    }

    #[test]
    fn snapshot_state_labels_are_bounded() {
        for state in [
            BootstrapState::Empty,
            BootstrapState::CacheSufficient,
            BootstrapState::ReseedRequired,
            BootstrapState::Reseeding,
            BootstrapState::ReadyForNetworkIntegration,
            BootstrapState::DegradedInsufficientPeers,
            BootstrapState::Failed,
        ] {
            let label = state.to_string();
            assert!(!label.is_empty());
            assert!(label.len() <= 64);
        }
        assert!(BootstrapState::Failed.is_terminal());
        assert!(BootstrapState::ReadyForNetworkIntegration.is_terminal());
        assert!(!BootstrapState::Empty.is_terminal());
    }

    #[test]
    fn store_summary_handles_empty_store() {
        let store = RouterInfoStore::default();
        let snapshot = store_summary(&store);
        assert_eq!(snapshot.record_count, 0);
        assert_eq!(snapshot.state, BootstrapState::Empty);
    }
}
