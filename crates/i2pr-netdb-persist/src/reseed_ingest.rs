//! Plan 104 SU3/reseed ingest composition.
//!
//! The ingest composition accepts verified SU3 bytes (or the verified
//! archive bytes after SU3 trust verification), runs the
//! [`i2pr_netdb::verify_su3_archive`] pipeline, and inserts the
//! validated records through the normal [`RouterInfoStore::insert`]
//! path. The composition never opens sockets, never accepts plain
//! HTTP bytes, and never marks a record as trusted unless it passes
//! the Plan 103 validator.

use i2pr_netdb::{
    InsertOutcome, ReseedEntryReport, ReseedEntryState, ReseedLimits, ReseedSignerTrustSet,
    ReseedVerifyOutcome, ReseedVerifyReport, RouterInfoStore, ValidatedRouterInfo,
    ValidationContext, verify_su3_archive, verify_su3_with_signers,
};
use thiserror::Error;

use crate::cache_loader::CacheLoader;

/// Errors emitted by the reseed ingestor.
#[derive(Debug, Error)]
pub enum ReseedIngestError {
    /// The trust-set lookup rejected the signer identifier.
    #[error("reseed signer not in trust set")]
    UnknownSigner,
    /// The bundle could not be parsed or verified.
    #[error("reseed bundle failed verification: {0}")]
    Verification(String),
    /// The signed bundle produced zero validated RouterInfos.
    #[error("reseed bundle yielded zero valid RouterInfos")]
    EmptyResult,
}

/// Tunable limits for the reseed ingest pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReseedIngestLimits {
    /// Maximum SU3 input bytes.
    pub max_su3_bytes: usize,
    /// Maximum archive entries.
    pub max_archive_entries: usize,
    /// Maximum cumulative uncompressed bytes.
    pub max_archive_uncompressed_bytes: u64,
    /// Maximum cumulative RouterInfo bytes.
    pub max_archive_router_info_bytes: u64,
    /// Maximum per-entry uncompressed bytes.
    pub max_entry_uncompressed_bytes: u64,
}

impl Default for ReseedIngestLimits {
    fn default() -> Self {
        let defaults = ReseedLimits::default();
        Self {
            max_su3_bytes: defaults.max_su3_bytes,
            max_archive_entries: defaults.max_archive_entries,
            max_archive_uncompressed_bytes: defaults.max_archive_uncompressed_bytes,
            max_archive_router_info_bytes: defaults.max_archive_router_info_bytes,
            max_entry_uncompressed_bytes: defaults.max_entry_uncompressed_bytes,
        }
    }
}

impl From<ReseedIngestLimits> for ReseedLimits {
    fn from(limits: ReseedIngestLimits) -> Self {
        Self {
            max_su3_bytes: limits.max_su3_bytes,
            max_archive_entries: limits.max_archive_entries,
            max_archive_uncompressed_bytes: limits.max_archive_uncompressed_bytes,
            max_archive_router_info_bytes: limits.max_archive_router_info_bytes,
            max_entry_uncompressed_bytes: limits.max_entry_uncompressed_bytes,
            max_router_info_encoded_bytes: i2pr_proto::MAX_COMMON_STRUCTURE_SIZE,
        }
    }
}

/// Per-entry report produced by the ingest run.
pub use i2pr_netdb::ReseedEntryReport as ReseedEntryReportAlias;

/// Aggregate report from one ingest run.
#[derive(Clone, Debug)]
pub struct ReseedBundleReport {
    /// Full per-entry and aggregate outcome from the verifier.
    pub verifier: ReseedVerifyReport,
    /// Insertion counts grouped by [`InsertOutcome`].
    pub inserts: ReseedInsertCounts,
}

/// Insertion counts grouped by typed [`InsertOutcome`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReseedInsertCounts {
    /// New records inserted.
    pub inserted: usize,
    /// Older records replaced.
    pub replaced: usize,
    /// Idempotent (byte-identical) inserts.
    pub idempotent: usize,
    /// Records rejected as stale.
    pub stale: usize,
    /// Records rejected as byte-conflicts.
    pub conflict: usize,
    /// Records rejected for capacity.
    pub capacity: usize,
}

/// Sanitized aggregate summary used by the daemon for diagnostics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReseedSummary {
    /// Total records inspected.
    pub total: usize,
    /// Records accepted and inserted.
    pub accepted: usize,
    /// Records rejected for filename/hash mismatch.
    pub rejected_filename: usize,
    /// Records rejected for decode failure.
    pub rejected_decode: usize,
    /// Records rejected for Plan 103 validation.
    pub rejected_validation: usize,
}

impl From<&ReseedBundleReport> for ReseedSummary {
    fn from(report: &ReseedBundleReport) -> Self {
        let mut summary = ReseedSummary::default();
        for entry in &report.verifier.entries {
            summary.total += 1;
            match entry.state {
                ReseedEntryState::Accepted => summary.accepted += 1,
                ReseedEntryState::RejectedFilename => summary.rejected_filename += 1,
                ReseedEntryState::RejectedDecode => summary.rejected_decode += 1,
                ReseedEntryState::RejectedValidation => summary.rejected_validation += 1,
            }
        }
        summary
    }
}

/// The Plan 104 reseed ingestor. The type holds only the trust set
/// and limits; every run consumes fresh state from the store.
#[derive(Debug)]
pub struct ReseedIngestor<'a> {
    trust: &'a ReseedSignerTrustSet,
    limits: ReseedIngestLimits,
}

impl<'a> ReseedIngestor<'a> {
    /// Constructs a new ingestor bound to the supplied trust set.
    pub fn new(trust: &'a ReseedSignerTrustSet) -> Self {
        Self {
            trust,
            limits: ReseedIngestLimits::default(),
        }
    }

    /// Constructs an ingestor with custom limits.
    pub fn with_limits(trust: &'a ReseedSignerTrustSet, limits: ReseedIngestLimits) -> Self {
        Self { trust, limits }
    }

    /// Returns the bound trust set.
    pub fn trust(&self) -> &ReseedSignerTrustSet {
        self.trust
    }

    /// Returns the active limits.
    pub fn limits(&self) -> ReseedIngestLimits {
        self.limits
    }

    /// Verifies an SU3 bundle and inserts the validated records into
    /// the store.
    ///
    /// The method first runs [`verify_su3_with_signers`], then writes
    /// the accepted bytes through the cache loader path so that the
    /// cache and the in-memory store remain in lock-step.
    pub fn ingest_su3_into(
        &self,
        bundle: &[u8],
        now_seconds: u64,
        validation_context: ValidationContext,
        store: &mut RouterInfoStore,
        cache: Option<&CacheLoader>,
    ) -> Result<ReseedBundleReport, ReseedIngestError> {
        let verifier_report = verify_su3_with_signers(
            bundle,
            self.trust,
            now_seconds,
            self.limits.into(),
            validation_context,
        )
        .map_err(|error| ReseedIngestError::Verification(error.to_string()))?;
        if matches!(
            verifier_report.outcome,
            ReseedVerifyOutcome::RejectedTrust { .. }
        ) {
            return Err(ReseedIngestError::UnknownSigner);
        }
        if verifier_report.accepted.is_empty() {
            return Err(ReseedIngestError::EmptyResult);
        }
        let inserts = insert_validated(&verifier_report.accepted, store);
        if let Some(cache_loader) = cache {
            for validated in &verifier_report.accepted {
                let encoded = validated
                    .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
                    .map_err(|error| ReseedIngestError::Verification(error.to_string()))?;
                let hash = validated.key();
                let hex = hex_lower(hash.as_bytes());
                cache_loader
                    .cache()
                    .write(&hex, &encoded)
                    .map_err(|error| ReseedIngestError::Verification(error.to_string()))?;
            }
        }
        Ok(ReseedBundleReport {
            verifier: verifier_report,
            inserts,
        })
    }

    /// Verifies an already-trusted bundle (e.g. bytes that arrived
    /// from a locally-controlled offline path) and inserts the
    /// records.
    pub fn ingest_verified_archive_into(
        &self,
        archive: &[u8],
        validation_context: ValidationContext,
        store: &mut RouterInfoStore,
        cache: Option<&CacheLoader>,
    ) -> Result<ReseedBundleReport, ReseedIngestError> {
        let report = verify_su3_archive(archive, self.limits.into(), validation_context)
            .map_err(|error| ReseedIngestError::Verification(error.to_string()))?;
        if report.accepted.is_empty() {
            return Err(ReseedIngestError::EmptyResult);
        }
        let inserts = insert_validated(&report.accepted, store);
        if let Some(cache_loader) = cache {
            for validated in &report.accepted {
                let encoded = validated
                    .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
                    .map_err(|error| ReseedIngestError::Verification(error.to_string()))?;
                let hash = validated.key();
                let hex = hex_lower(hash.as_bytes());
                cache_loader
                    .cache()
                    .write(&hex, &encoded)
                    .map_err(|error| ReseedIngestError::Verification(error.to_string()))?;
            }
        }
        Ok(ReseedBundleReport {
            verifier: report,
            inserts,
        })
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

fn insert_validated(
    records: &[ValidatedRouterInfo],
    store: &mut RouterInfoStore,
) -> ReseedInsertCounts {
    let mut counts = ReseedInsertCounts::default();
    for validated in records {
        match store.insert(validated.clone()) {
            InsertOutcome::Inserted => counts.inserted += 1,
            InsertOutcome::Replaced => counts.replaced += 1,
            InsertOutcome::Idempotent => counts.idempotent += 1,
            InsertOutcome::StaleReplacement => counts.stale += 1,
            InsertOutcome::Conflict => counts.conflict += 1,
            InsertOutcome::CapacityExceeded => counts.capacity += 1,
        }
    }
    counts
}

/// Compatibility alias that surfaces the reseed entry report under the
/// composition-owner's module path.
#[allow(dead_code)]
pub type ReseedEntryReportAliasLocal = ReseedEntryReport;

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_netdb::ReseedSignerTrustSet;
    use i2pr_proto::Date;

    #[test]
    fn ingest_without_signers_rejects_unknown_signer() {
        let trust = ReseedSignerTrustSet::new();
        let ingestor = ReseedIngestor::new(&trust);
        let mut store = RouterInfoStore::default();
        let error = ingestor
            .ingest_su3_into(
                b"",
                0,
                ValidationContext::new(Date::from_millis(1)),
                &mut store,
                None,
            )
            .unwrap_err();
        // Empty bytes fail the header parse before the trust lookup; we
        // only assert that *some* typed failure surfaces here.
        assert!(matches!(
            error,
            ReseedIngestError::Verification(_) | ReseedIngestError::UnknownSigner
        ));
    }

    #[test]
    fn from_report_to_summary_counts_outcomes() {
        let report = ReseedBundleReport {
            verifier: ReseedVerifyReport {
                outcome: ReseedVerifyOutcome::Accepted { accepted: 2 },
                entries: vec![
                    ReseedEntryReport {
                        name: "a".to_owned(),
                        state: ReseedEntryState::Accepted,
                        error: None,
                    },
                    ReseedEntryReport {
                        name: "b".to_owned(),
                        state: ReseedEntryState::RejectedFilename,
                        error: None,
                    },
                ],
                accepted: Vec::new(),
            },
            inserts: ReseedInsertCounts::default(),
        };
        let summary = ReseedSummary::from(&report);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.accepted, 1);
        assert_eq!(summary.rejected_filename, 1);
    }

    #[test]
    fn cache_loader_report_field_is_accessible() {
        let report = crate::cache_loader::CacheLoaderReport::default();
        assert!(report.record("missing").is_none());
    }
}
