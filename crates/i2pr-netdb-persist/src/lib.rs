//! Composition owner for the Plan 104 NetDB cache loader and SU3 reseed
//! ingestion.
//!
//! `i2pr-netdb-persist` owns the seam between the bounded raw-byte cache
//! in `i2pr-storage` and the runtime-neutral `i2pr-netdb` validator
//! and store. It deliberately sits above both crates: it does not embed
//! validation, hashing, RouterInfo decoding, or filesystem policies;
//! it composes the existing narrow APIs.
//!
//! Public entry points are documented on each module:
//!
//! - [`cache_loader`] loads and revalidates cached RouterInfo bytes
//!   into a `RouterInfoStore`.
//! - [`reseed_ingest`] ingests verified SU3/reseed bytes through the
//!   Plan 103 validator into a `RouterInfoStore`.
//!
//! This crate is the canonical composition owner for Plan 104; Plan 105
//! and Plan 106 consume its typed APIs without reaching into the
//! filesystem.

#![forbid(unsafe_code)]

pub mod cache_loader;
pub mod reseed_ingest;

pub use cache_loader::{
    CacheLoader, CacheLoaderLimits, CacheLoaderReport, CacheLoaderScanBudget, LoadedCacheRecord,
    LoadedCacheState,
};
pub use i2pr_netdb::ReseedEntryReport;
pub use reseed_ingest::{
    ReseedBundleReport, ReseedIngestLimits, ReseedIngestor, ReseedInsertCounts, ReseedSummary,
};
