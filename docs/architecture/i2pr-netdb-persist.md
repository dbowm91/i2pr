# `i2pr-netdb-persist` — Deep Dive

Composition owner for the Plan 104 persistent RouterInfo cache and the
Plan 103 SU3 reseed ingestion. Sits **above** `i2pr-netdb`
(validation) and `i2pr-storage` (byte-level cache seam); it never
embeds validation, hashing, RouterInfo decoding, or filesystem
policies, and it composes only the existing narrow APIs.

Path: `crates/i2pr-netdb-persist/`

## Purpose

The crate is the canonical composition owner for two pipelines:

1. **Persistent cache loading** — read raw byte records from the
   on-disk cache, decode each through the Plan 103 `RouterInfo`
   codec, validate through `ValidatedRouterInfo`, and insert through
   the normal `RouterInfoStore::insert` path. Disk bytes are never
   deserialized into a trusted wrapper directly.
2. **SU3 reseed ingestion** — accept verified SU3/reseed bytes
   through the Plan 103 validator and write them into a
   `RouterInfoStore`.

`Plan 105` (router-info validation helpers) and `Plan 106` (NetDB
bootstrap pipeline) consume the typed APIs published here without
reaching into the filesystem. The daemon's
`i2pr-daemon::bootstrap` module and `NetDbSeam` use these entry
points.

Failure policy across both pipelines:

- A bad cache file or bad reseed entry is **isolated**; one corrupt
  entry cannot make the rest of the corpus unavailable.
- Explicit per-file, aggregate-byte, and entry-count budgets are
  enforced at the composition layer.
- Aggregate counts only are surfaced through the typed reports.
- No network, no DNS, no sockets, no public-network calls. The
  HTTPS reseed adapter is deferred to a future plan; the offline
  SU3 source path is the only allowlisted acquisition path.

## Module layout

Flat — three files at the crate root:

| File | Responsibility | Main items |
| --- | --- | --- |
| `src/lib.rs` | Crate root: re-exports + module wiring | `pub mod cache_loader;`, `pub mod reseed_ingest;` |
| `src/cache_loader.rs` | Plan 104 persistent RouterInfo cache loader | `CacheLoader`, `CacheLoaderLimits`, `CacheLoaderScanBudget`, `LoadedCacheRecord`, `LoadedCacheState`, `CacheLoaderReport`, `CacheLoaderError` |
| `src/reseed_ingest.rs` | Plan 103 SU3 reseed ingestion | `ReseedIngestor`, `ReseedIngestLimits`, `ReseedBundleReport`, `ReseedInsertCounts`, `ReseedSummary`, `ReseedIngestError` |

`crates/i2pr-netdb-persist/src/lib.rs` also re-exports
`i2pr_netdb::ReseedEntryReport` for downstream callers.

## Public surface

### Crate root (`src/lib.rs`)

```rust
pub mod cache_loader;
pub mod reseed_ingest;

pub use cache_loader::{
    CacheLoader, CacheLoaderLimits, CacheLoaderReport, CacheLoaderScanBudget,
    LoadedCacheRecord, LoadedCacheState,
};
pub use i2pr_netdb::ReseedEntryReport;
pub use reseed_ingest::{
    ReseedBundleReport, ReseedIngestLimits, ReseedIngestor, ReseedInsertCounts,
    ReseedSummary,
};
```

### `src/cache_loader.rs`

- `enum CacheLoaderError` (`cache_loader.rs:30`):
  - `Cache { operation, source: CacheError }`
  - `ScanBudget(String)`
  - `Prepare(String)`
- `struct CacheLoaderLimits` (`cache_loader.rs:50`) — bounded
  `max_entries` and `max_bytes`. `Default` derives from the
  cache-seam ceilings `MAX_CACHE_SCAN_ENTRIES = 16384` and
  `MAX_CACHE_SCAN_BYTES = 32 MiB`.
- `struct CacheLoaderScanBudget` (`cache_loader.rs:70`) —
  checked-arithmetic tracker for entries and bytes. `new`,
  `limits`, `entries_seen`, `bytes_seen`, `record`.
- `struct LoadedCacheRecord` (`cache_loader.rs:132`) — per-file
  report (`name`, `state`).
- `enum LoadedCacheState` (`cache_loader.rs:141`) —
  `Inserted { outcome: InsertOutcome }` / `Invalid { error }` /
  `Unreadable { reason }`.
- `struct CacheLoaderReport` (`cache_loader.rs:162`) — aggregate
  report. `records`, `entries_inspected`, `bytes_inspected`,
  `inserted`, `invalid`, `unreadable`, `record(name)`.
- `struct CacheLoader` (`cache_loader.rs:188`) — stateless beyond
  the supplied cache. `new(cache: ByteCache)`, `cache()`, and a
  bounded loader entry point that reads, decodes, validates, and
  inserts each entry through `RouterInfoStore::insert`.

### `src/reseed_ingest.rs`

- `enum ReseedIngestError` (`reseed_ingest.rs:22`) — typed
  ingestion categories.
- `struct ReseedIngestLimits` (`reseed_ingest.rs:36`) — per-run
  caps on bundle size, entry count, decoded bytes.
- `struct ReseedBundleReport` (`reseed_ingest.rs:80`) — per-bundle
  outcome.
- `struct ReseedInsertCounts` (`reseed_ingest.rs:89`) — typed
  insert counts (new, replacement, invalid, malformed).
- `struct ReseedSummary` (`reseed_ingest.rs:106`) — aggregate
  ingest summary.
- `struct ReseedIngestor<'a>` (`reseed_ingest.rs:138`) —
  short-lived, lifetime-bound ingestor that borrows the target
  `RouterInfoStore` and a verifier. The lifetime keeps the
  composition safe by construction.

## Composition contracts

The two pipelines share the same boundary:

```
        raw bytes            validated bytes
[disk] ---------> [i2pr-storage] -> [i2pr-netdb] --------> [RouterInfoStore]
                  cache_seam       Validator /
                  (ByteCache)      RouterInfo codec
                       |
                       v
              i2pr-netdb-persist
              (this crate: orchestration + budgets +
               typed reports)
```

The loader and the ingestor are the **only** consumers of the
`ByteCache` seam that the daemon runs through. They:

1. Open or scan the cache through `ByteCache::prepare` /
   `ByteCache::scan` / `ByteCache::read`.
2. Bound the scan through `CacheLoaderScanBudget`.
3. Decode bytes through `i2pr_proto::RouterInfo` (no
   deserialize-into-trusted).
4. Validate through `i2pr_netdb::ValidatedRouterInfo` /
   `RouterInfoStore::insert`.
5. Surface typed outcomes through `CacheLoaderReport` /
   `ReseedSummary`.

Disk bytes are never trusted: every step has an explicit typed
boundary. `CacheLoaderError::Cache` propagates the cache-seam
rejection with the original `CacheError` source.

## Dependencies

| Dependency | Source | Purpose |
| --- | --- | --- |
| `i2pr-crypto` | path | Hash primitives consumed transitively via `i2pr-netdb` validation |
| `i2pr-netdb` | path | `RouterInfoStore`, `ValidatedRouterInfo`, `ValidationContext`, `InsertOutcome`, `RouterHash`, `ReseedEntryReport` |
| `i2pr-proto` | path | `RouterInfo` codec |
| `i2pr-storage` | path | `cache_seam::ByteCache`, `cache_seam::CacheError`, scan ceiling constants |
| `thiserror` | workspace | `CacheLoaderError`, `ReseedIngestError` derives |
| `rand_chacha` (dev) | workspace | Deterministic reseed ingestion tests |
| `rand_core` (dev) | workspace | Deterministic reseed ingestion tests |
| `tempfile` (dev) | workspace | Cache fixture tests |

Dependency chain:
`i2pr-proto ← i2pr-crypto ← i2pr-storage` →
`i2pr-netdb` → `i2pr-netdb-persist`. `i2pr-daemon` is the only
production consumer.

## Distinctive design choices

1. **Composition only** — no validation, no RouterInfo decoding, no
   hashing, no filesystem policy. Every step is delegated to a
   narrower crate that owns its concern.
2. **Bad-record isolation** — a corrupt cache file or malformed
   reseed entry never poisons the rest of the corpus.
3. **Aggregate-only reporting** — the `CacheLoaderReport` and
   `ReseedSummary` surface counts only, never raw bytes or
   identities.
4. **No-replace `save_new` discipline is inherited** from
   `i2pr-storage` via the `ByteCache` seam; the loader never
   mutates a record that is already valid.
5. **`ReseedIngestor<'a>` is lifetime-bound to the store** — the
   composition cannot outlive the store it borrows.

## Cross-references

- [Overview](overview.md)
- [i2pr-netdb](i2pr-netdb.md) — owns the validator and store
  surface that this crate composes.
- [i2pr-storage](i2pr-storage.md) — owns the byte-level
  `cache_seam::ByteCache` this crate reads and writes through.
- [i2pr-daemon](i2pr-daemon.md) — consumes these entry points
  during `bootstrap_daemon` and via `NetDbSeam`.
- Plan-of-record: `plans/104-*.md`; closure
  `plans/104-status.md` (cache loading) and
  `plans/105-status.md` (reseed composition).