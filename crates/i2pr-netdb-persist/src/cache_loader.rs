//! Plan 104 bounded persistent RouterInfo cache loader.
//!
//! The loader reads raw byte records from the
//! [`i2pr_storage::cache_seam::ByteCache`], decodes each one through
//! the Plan 103 [`RouterInfo`] codec, validates it through the
//! [`ValidatedRouterInfo`] boundary, and inserts the result through
//! the normal [`RouterInfoStore::insert`] path. Disk bytes are never
//! deserialized into a trusted wrapper directly.
//!
//! Failure policy:
//!
//! - bad cache files are isolated; one corrupt entry cannot make the
//!   rest of the cache unavailable;
//! - the loader enforces explicit per-file, aggregate-byte, and
//!   entry-count scan budgets;
//! - aggregate counts only are surfaced through the report.
//!
//! The loader does not touch NTCP2, does not open any socket, and
//! does not download anything from the network.

use i2pr_netdb::{
    InsertOutcome, RouterHash, RouterInfoStore, ValidatedRouterInfo, ValidationContext,
};
use i2pr_proto::RouterInfo;
use i2pr_storage::cache_seam::{ByteCache, CacheError};
use thiserror::Error;

/// Errors returned by the Plan 104 cache loader.
#[derive(Debug, Error)]
pub enum CacheLoaderError {
    /// The cache seam rejected the operation.
    #[error("cache seam {operation} failed: {source}")]
    Cache {
        /// Static filesystem operation category.
        operation: &'static str,
        /// Underlying cache error.
        #[source]
        source: CacheError,
    },
    /// Aggregate or per-file scan budget exceeded.
    #[error("cache loader scan budget exceeded: {0}")]
    ScanBudget(String),
    /// I/O error during cache preparation.
    #[error("cache preparation failed: {0}")]
    Prepare(String),
}

/// Tunable limits for a single cache loader run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheLoaderLimits {
    /// Maximum entries inspected per loader run. Defaults to the seam
    /// ceiling so the loader does not advance past the budget.
    pub max_entries: usize,
    /// Maximum cumulative bytes inspected per loader run.
    pub max_bytes: u64,
}

impl Default for CacheLoaderLimits {
    fn default() -> Self {
        Self {
            max_entries: i2pr_storage::cache_seam::MAX_CACHE_SCAN_ENTRIES,
            max_bytes: i2pr_storage::cache_seam::MAX_CACHE_SCAN_BYTES,
        }
    }
}

/// A bounded scan-budget wrapper used by the loader to fail closed at
/// the configured limits without retrying.
#[derive(Debug)]
pub struct CacheLoaderScanBudget {
    entries_seen: usize,
    bytes_seen: u64,
    limits: CacheLoaderLimits,
}

impl CacheLoaderScanBudget {
    /// Constructs a fresh budget tracker for the supplied limits.
    pub fn new(limits: CacheLoaderLimits) -> Self {
        Self {
            entries_seen: 0,
            bytes_seen: 0,
            limits,
        }
    }

    /// Returns the configured limits.
    pub fn limits(&self) -> CacheLoaderLimits {
        self.limits
    }

    /// Returns the current entry-count expenditure.
    pub fn entries_seen(&self) -> usize {
        self.entries_seen
    }

    /// Returns the current byte expenditure.
    pub fn bytes_seen(&self) -> u64 {
        self.bytes_seen
    }

    /// Records the inspection of a file whose declared length is
    /// `length`. Returns `Err` if the limit would be exceeded.
    pub fn record(&mut self, length: u64) -> Result<(), CacheLoaderError> {
        let next_entries = self
            .entries_seen
            .checked_add(1)
            .ok_or_else(|| CacheLoaderError::ScanBudget("entry counter overflow".to_owned()))?;
        if next_entries > self.limits.max_entries {
            return Err(CacheLoaderError::ScanBudget(format!(
                "entries {next_entries} exceed {}",
                self.limits.max_entries
            )));
        }
        let next_bytes = self
            .bytes_seen
            .checked_add(length)
            .ok_or_else(|| CacheLoaderError::ScanBudget("byte counter overflow".to_owned()))?;
        if next_bytes > self.limits.max_bytes {
            return Err(CacheLoaderError::ScanBudget(format!(
                "bytes {next_bytes} exceed {}",
                self.limits.max_bytes
            )));
        }
        self.entries_seen = next_entries;
        self.bytes_seen = next_bytes;
        Ok(())
    }
}

/// One per-file report produced by a loader run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedCacheRecord {
    /// Cache filename.
    pub name: String,
    /// Outcome of the loader pass for this file.
    pub state: LoadedCacheState,
}

/// Per-file outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadedCacheState {
    /// The file was read, validated, and inserted into the store.
    Inserted {
        /// Insert outcome for diagnostics.
        outcome: InsertOutcome,
    },
    /// The file was read and decoded but the cached RouterInfo did not
    /// validate through Plan 103.
    Invalid {
        /// Typed validation category.
        error: String,
    },
    /// The file was unreadable or rejected by the cache seam.
    Unreadable {
        /// Reason for the rejection.
        reason: String,
    },
}

/// Aggregate report from one loader run.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CacheLoaderReport {
    /// Per-file outcomes.
    pub records: Vec<LoadedCacheRecord>,
    /// Total entries inspected.
    pub entries_inspected: usize,
    /// Total bytes inspected.
    pub bytes_inspected: u64,
    /// Number of records inserted (new or replacement).
    pub inserted: usize,
    /// Number of records that failed validation.
    pub invalid: usize,
    /// Number of records that were unreadable or rejected by the
    /// cache seam.
    pub unreadable: usize,
}

impl CacheLoaderReport {
    /// Returns the typed outcome for `name`, if any.
    pub fn record(&self, name: &str) -> Option<&LoadedCacheRecord> {
        self.records.iter().find(|record| record.name == name)
    }
}

/// The Plan 104 cache loader. The type is stateless beyond the
/// supplied cache seam; every run consumes fresh state from the store.
#[derive(Debug)]
pub struct CacheLoader {
    cache: ByteCache,
}

impl CacheLoader {
    /// Creates a cache loader rooted at the supplied data directory.
    pub fn new(cache: ByteCache) -> Self {
        Self { cache }
    }

    /// Returns the underlying cache seam.
    pub fn cache(&self) -> &ByteCache {
        &self.cache
    }

    /// Scans the cache directory and revalidates every file through the
    /// Plan 103 validator before insertion.
    ///
    /// The method is fail-closed: a corrupt entry does not erase
    /// previously inserted valid records, and aggregate failure counts
    /// are surfaced through [`CacheLoaderReport`].
    pub fn load_into(
        &self,
        store: &mut RouterInfoStore,
        validation_context: ValidationContext,
    ) -> Result<CacheLoaderReport, CacheLoaderError> {
        self.load_into_with_limits(store, validation_context, CacheLoaderLimits::default())
    }

    /// Same as [`Self::load_into`] but with custom scan budgets.
    pub fn load_into_with_limits(
        &self,
        store: &mut RouterInfoStore,
        validation_context: ValidationContext,
        limits: CacheLoaderLimits,
    ) -> Result<CacheLoaderReport, CacheLoaderError> {
        if !self.cache.exists() {
            // An empty/missing cache is a valid bootstrap state.
            return Ok(CacheLoaderReport::default());
        }
        self.cache
            .prepare()
            .map_err(|error| CacheLoaderError::Prepare(error.to_string()))?;
        let names = self
            .cache
            .scan()
            .map_err(|error| CacheLoaderError::Prepare(error.to_string()))?;
        let mut budget = CacheLoaderScanBudget::new(limits);
        let mut report = CacheLoaderReport::default();
        for name in names {
            let bytes = match self.cache.read(&name) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => continue,
                Err(error) => {
                    report.unreadable += 1;
                    report.records.push(LoadedCacheRecord {
                        name: name.clone(),
                        state: LoadedCacheState::Unreadable {
                            reason: error.to_string(),
                        },
                    });
                    continue;
                }
            };
            if budget.record(bytes.len() as u64).err().is_some() {
                return Err(CacheLoaderError::ScanBudget(format!(
                    "scan budget exhausted while processing {name}"
                )));
            }
            report.entries_inspected += 1;
            report.bytes_inspected = report.bytes_inspected.saturating_add(bytes.len() as u64);
            let info = match RouterInfo::decode(&bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE) {
                Ok(info) => info,
                Err(error) => {
                    report.invalid += 1;
                    report.records.push(LoadedCacheRecord {
                        name: name.clone(),
                        state: LoadedCacheState::Invalid {
                            error: format!("decode: {error}"),
                        },
                    });
                    continue;
                }
            };
            let expected_key = router_info_name_hash(&name);
            let validated =
                match ValidatedRouterInfo::from_router_info(info, expected_key, validation_context)
                {
                    Ok(validated) => validated,
                    Err(error) => {
                        report.invalid += 1;
                        report.records.push(LoadedCacheRecord {
                            name: name.clone(),
                            state: LoadedCacheState::Invalid {
                                error: format!("validate: {error}"),
                            },
                        });
                        continue;
                    }
                };
            let outcome = store.insert(validated);
            if matches!(outcome, InsertOutcome::Inserted | InsertOutcome::Replaced) {
                report.inserted += 1;
            }
            report.records.push(LoadedCacheRecord {
                name: name.clone(),
                state: LoadedCacheState::Inserted { outcome },
            });
        }
        Ok(report)
    }
}

fn router_info_name_hash(name: &str) -> Option<RouterHash> {
    let stem = name
        .strip_suffix(".ri")
        .or_else(|| name.strip_suffix(".b32"))
        .unwrap_or(name);
    if stem.len() != 64 || !stem.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut hash = [0_u8; 32];
    for (index, pair) in stem.as_bytes().chunks(2).enumerate() {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        hash[index] = ((high << 4) | low) as u8;
    }
    Some(RouterHash::from_bytes(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_netdb::RouterInfoStore;
    use i2pr_proto::Date;
    use tempfile::tempdir;

    fn data_dir_with_parents(directory: &tempfile::TempDir) -> std::path::PathBuf {
        let data_dir = directory.path().join("state");
        i2pr_storage::IdentityStore::prepare_directory(&data_dir).expect("private dir");
        // `cache.prepare()` calls `ensure_secure_directory` which only
        // inspects the immediate parent; pre-create the intermediate
        // directories so the cache root can be created. The cache
        // seam enforces 0o700 on every directory it owns, so the
        // intermediate parents must also satisfy that bound.
        for sub in ["netdb", "netdb/routers"] {
            let path = data_dir.join(sub);
            std::fs::create_dir_all(&path).expect("intermediate dir");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                    .expect("private perms");
            }
        }
        data_dir
    }

    #[test]
    fn missing_cache_is_empty_report() {
        let directory = tempdir().expect("directory");
        let data_dir = data_dir_with_parents(&directory);
        let cache = ByteCache::in_data_dir(&data_dir);
        let loader = CacheLoader::new(cache);
        let mut store = RouterInfoStore::default();
        let report = loader
            .load_into(&mut store, ValidationContext::new(Date::from_millis(1)))
            .expect("missing cache is ok");
        assert_eq!(report.entries_inspected, 0);
        assert!(store.is_empty());
    }

    #[test]
    fn cache_loader_rejects_unknown_filename() {
        let directory = tempdir().expect("directory");
        let data_dir = data_dir_with_parents(&directory);
        let cache = ByteCache::in_data_dir(&data_dir);
        cache.prepare().expect("prepare");
        // Use a syntactically valid 64-lowercase-hex filename that
        // does not match any actual RouterHash. The cache loader must
        // decode, find no match for the contained RouterIdentity, and
        // record a typed validation failure.
        let bogus_name = "ab".repeat(32);
        cache.write(&bogus_name, b"garbage bytes").expect("write");
        let loader = CacheLoader::new(cache);
        let mut store = RouterInfoStore::default();
        let report = loader
            .load_into(&mut store, ValidationContext::new(Date::from_millis(1)))
            .expect("loader runs and reports bad entries");
        assert_eq!(report.unreadable + report.invalid, 1);
    }
}
