# `i2pr-netdb-persist` — Deep Dive

Composition owner for the Plan 104 persistent RouterInfo cache, the Plan 104
SU3/reseed ingest composition, and the Plan 276 versioned floodfill durable
envelope.

Path: `crates/i2pr-netdb-persist/`

## Purpose

`i2pr-netdb-persist` owns the seam between the bounded raw-byte cache in
`i2pr-storage` and the runtime-neutral validator/store in `i2pr-netdb`. Its
crate doc comment is explicit about the placement
(`src/lib.rs:6-8`): it *"deliberately sits above both crates: it does not
embed validation, hashing, RouterInfo decoding, or filesystem policies; it
composes the existing narrow APIs."* Three pipelines live here, and only
here:

1. **Persistent cache loading** (`cache_loader`) — scan raw byte records
   from the on-disk `ByteCache`, decode each through the `i2pr_proto::RouterInfo`
   codec, revalidate through `ValidatedRouterInfo`, and insert through
   `RouterInfoStore::insert`.
2. **SU3/reseed ingestion** (`reseed_ingest`) — run verified SU3 bytes
   through the Plan 103 `i2pr-netdb` reseed verifier and insert the accepted
   records, optionally mirroring them into the same cache.
3. **Floodfill durable records** (`floodfill_records`) — encode/decode the
   versioned, SHA-256-checksummed `I2FF` envelope and revalidate every
   restored payload before it re-enters the NetDB.

What it must **not** own, and where that work actually lives:

| Concern | Real owner |
| --- | --- |
| Raw-byte cache, scan ceilings, atomic replacement, `0o700` directory policy | `i2pr-storage` — `cache_seam::ByteCache` |
| RouterInfo decode/validate, `RouterInfoStore`/`ServerNetDb`, provenance policy, the `I2FF` payload revalidation primitives | `i2pr-netdb` |
| SU3 container framing and RSA-SHA512 signature math | `i2pr-su3` |
| Reseed ZIP entry/expansion limits, signer trust set, cert-validity and publication checks | `i2pr-netdb` — `src/reseed.rs` |
| Wiring the pipelines into a running router, listeners, timers, sockets | `i2pr-daemon` (`src/bootstrap.rs`, `src/netdb_seam.rs`) |

Failure policy, uniform across all three pipelines:

- A bad cache file, a malformed reseed entry, or a corrupt envelope is
  **isolated**; one bad record cannot make the rest of the corpus unavailable.
- Explicit per-file, aggregate-byte, entry-count, and expansion budgets are
  enforced at the composition layer.
- Aggregate counts only are surfaced through the typed reports.
- No network, no DNS, no sockets, no public-network calls, no clock. The
  HTTPS reseed adapter does not exist; the caller supplies already-fetched
  bytes. The offline SU3 source path is the only acquisition path.

## Module layout

Flat module layout; no `benches/`, no `fixtures/`, no `tests/` directory.

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/lib.rs` | 39 | Crate doc, module wiring, curated re-exports | — (re-exports only) |
| `src/cache_loader.rs` | 379 (1–316 production, 318–379 `#[cfg(test)]`) | Plan 104 bounded persistent RouterInfo cache loader: scan budget, per-file isolation, revalidate-then-insert | `CacheLoader`, `CacheLoaderError`, `CacheLoaderLimits`, `CacheLoaderScanBudget`, `CacheLoaderReport`, `LoadedCacheRecord`, `LoadedCacheState` |
| `src/floodfill_records.rs` | 525 (1–291 production, 293–525 `#[cfg(test)]`) | Plan 276 versioned, checksummed durable envelope; mandatory decode+revalidate on restore; provenance narrowing | `FLOODFILL_ENVELOPE_VERSION`, `FloodfillRecordEnvelope`, `FloodfillRecordStore`, `PersistError`, `PersistedIngress`, `PersistedPurpose` |
| `src/reseed_ingest.rs` | 347 (1–285 production, 287–347 `#[cfg(test)]`) | Plan 104 SU3/reseed ingest composition: trust-set dispatch, optional cache mirror, typed counts | `ReseedBundleReport`, `ReseedIngestError`, `ReseedIngestLimits`, `ReseedIngestor`, `ReseedInsertCounts`, `ReseedSummary`, `ReseedEntryReportAlias`, `ReseedEntryReportAliasLocal` |

## Public surface

### Crate root (`src/lib.rs:25-39`)

```rust
pub mod cache_loader;
pub mod floodfill_records;
pub mod reseed_ingest;

pub use cache_loader::{
    CacheLoader, CacheLoaderLimits, CacheLoaderReport, CacheLoaderScanBudget, LoadedCacheRecord,
    LoadedCacheState,
};
pub use floodfill_records::{
    FloodfillRecordEnvelope, FloodfillRecordStore, PersistError, PersistedPurpose,
};
pub use i2pr_netdb::ReseedEntryReport;
pub use reseed_ingest::{
    ReseedBundleReport, ReseedIngestLimits, ReseedIngestor, ReseedInsertCounts, ReseedSummary,
};
```

`CacheLoaderError`, `PersistedIngress`, and `FLOODFILL_ENVELOPE_VERSION` are
public at their module path but are **not** re-exported at the root; the
root additionally re-exports `i2pr_netdb::ReseedEntryReport` so downstream
callers need not depend on `i2pr-netdb` for that type.

### `src/cache_loader.rs`

| Item | Kind / source |
| --- | --- |
| `CacheLoaderError` | `enum` — `Cache { operation: &'static str, source: CacheError }` (`:30`), `ScanBudget(String)` (`:42`), `Prepare(String)` (`:45`). `CacheError` is `#[source]`-chained, never stringly-erased |
| `CacheLoaderLimits` | struct — `max_entries: usize`, `max_bytes: u64` (`:50`). `Default` derives from the cache-seam ceilings (`:58-65`) |
| `CacheLoaderScanBudget` | struct — checked-arithmetic tracker (`:70`); `new`, `limits`, `entries_seen`, `bytes_seen`, `record(length) -> Result<(), CacheLoaderError>` (`:103`) |
| `LoadedCacheRecord` | struct — `name: String`, `state: LoadedCacheState` (`:132`) |
| `LoadedCacheState` | `enum` — `Inserted { outcome: InsertOutcome }`, `Invalid { error: String }`, `Unreadable { reason: String }` (`:141`) |
| `CacheLoaderReport` | struct — `records`, `entries_inspected`, `bytes_inspected`, `inserted`, `invalid`, `unreadable` (`:162`); `record(name)` accessor (`:180`) |
| `CacheLoader` | struct holding one `ByteCache` (`:188`); `new(cache)` (`:194`), `cache()` (`:199`), `load_into(store, context)` (`:209`), `load_into_with_limits(store, context, limits)` (`:218`) |

### `src/reseed_ingest.rs`

| Item | Kind / source |
| --- | --- |
| `ReseedIngestError` | `enum` — `UnknownSigner` (`:25`), `Verification(String)` (`:28`), `EmptyResult` (`:31`) |
| `ReseedIngestLimits` | struct — `max_su3_bytes`, `max_archive_entries`, `max_archive_uncompressed_bytes`, `max_archive_router_info_bytes`, `max_entry_uncompressed_bytes` (`:36`); `Default` mirrors `i2pr_netdb::ReseedLimits::default()` (`:49-60`); `From<ReseedIngestLimits> for ReseedLimits` pins `max_router_info_encoded_bytes` to `i2pr_proto::MAX_COMMON_STRUCTURE_SIZE` (`:62-73`) |
| `ReseedEntryReportAlias` | `pub use i2pr_netdb::ReseedEntryReport` (`:76`) — surfaces the per-entry report under the composition owner's module path |
| `ReseedEntryReportAliasLocal` | `pub type … = ReseedEntryReport`, `#[allow(dead_code)]` compatibility alias (`:285`) |
| `ReseedBundleReport` | struct — `verifier: ReseedVerifyReport`, `inserts: ReseedInsertCounts` (`:80`) |
| `ReseedInsertCounts` | struct — `inserted`, `replaced`, `idempotent`, `stale`, `conflict`, `capacity` (`:89`), mapped one-to-one from the six `InsertOutcome` variants (`:270-277`) |
| `ReseedSummary` | struct — `total`, `accepted`, `rejected_filename`, `rejected_decode`, `rejected_validation` (`:106`); `From<&ReseedBundleReport>` (`:119`) folds the four `ReseedEntryState` variants |
| `ReseedIngestor<'a>` | struct borrowing `&'a ReseedSignerTrustSet` plus owned limits (`:138`); `new` (`:145`), `with_limits` (`:153`), `trust()` (`:158`), `limits()` (`:163`), `ingest_su3_into(bundle, now_seconds, context, store, cache: Option<&CacheLoader>)` (`:173`), `ingest_verified_archive_into(archive, context, store, cache)` (`:221`) |

### `src/floodfill_records.rs`

| Item | Kind / source |
| --- | --- |
| `FLOODFILL_ENVELOPE_VERSION` | `pub const u8 = 1` (`:18`) |
| `PersistedPurpose` | `enum` — `PublishedStore`, `FloodReplica`, `LocalPublication` (`:23`) |
| `PersistedIngress` | `enum` — `DirectPeer`, `RouterTunnel`, `Local` (`:30`) |
| `FloodfillRecordEnvelope` | struct — `record_type: u8`, `key: Hash`, `observed_at_ms: u64`, `ingress`, `purpose`, `canonical_bytes: Vec<u8>` (`:37`). Hand-written `Debug` (`:46`) redacts `key` and `canonical_bytes` as `[redacted]` |
| `PersistError` | `enum` — `Cache(CacheError)` (`:61`), `InvalidEnvelope` (`:63`), `UnsupportedType(u8)` (`:65`), `TooLarge` (`:67`), `InvalidRecord` (`:69`), `Expired` (`:71`), `Capacity` (`:73`) |
| `FloodfillRecordEnvelope::encode` | `encode(&self, max_bytes) -> Result<Vec<u8>, PersistError>` (`:77`) |
| `FloodfillRecordEnvelope::decode` | `decode(bytes: &[u8], max_bytes: usize, key: Hash) -> Result<Self, PersistError>` (`:109`) — the `key` argument is the caller's expected key, bound-checked against the header |
| `FloodfillRecordStore` | struct holding a `ByteCache` + caller-supplied `max_record_bytes` (`:172`); `new` (`:178`), `save(name, record)` (`:184`), `save_router_info(name, &ValidatedRouterInfo, observed_at_ms, purpose)` (`:189`), `load(name, key) -> Result<Option<_>, _>` (`:212`), `load_router_info_into(name, key, &mut ServerNetDb, context, now_ms, max_observed_age_ms)` (`:225`), `load_validated_into(name, key, &mut ServerNetDb, now_ms, max_observed_age_ms, validate)` (`:256`) |

## Key contracts

### 1. `cache_loader` — decode → validate → insert, disk is never trusted

`CacheLoader::load_into_with_limits` (`cache_loader.rs:218`) is a
synchronous, fail-closed scan:

1. **Missing cache is a valid bootstrap state** — `!self.cache.exists()`
   returns `CacheLoaderReport::default()` (`:224-227`); it is not an error.
2. `ByteCache::prepare()` then `ByteCache::scan()`; both seam errors map to
   `CacheLoaderError::Prepare` (`:228-234`).
3. For each name: `ByteCache::read` — `Ok(None)` is skipped silently; a read
   error increments `unreadable` and records `LoadedCacheState::Unreadable`
   (`:238-251`).
4. `CacheLoaderScanBudget::record(bytes.len())` charges the budget. **A budget
   overrun aborts the whole run** with `CacheLoaderError::ScanBudget`
   (`:252-256`) — unlike a bad record, which only skips.
5. `RouterInfo::decode(&bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)` — a
   decode failure increments `invalid` and records
   `Invalid { error: "decode: …" }` (`:259-271`).
6. **Key binding**: the cache filename stem (`.ri`/`.b32` suffix stripped) must
   be 64 ASCII hex digits, else the expected key is `None` and
   `ValidatedRouterInfo::from_router_info` cannot bind a hash
   (`router_info_name_hash`, `:301-316`). This is why a file whose name does
   not match its contained identity fails validation rather than being
   inserted under a filename-derived key.
7. `ValidatedRouterInfo::from_router_info(info, expected_key, context)` — a
   failure increments `invalid` and records
   `Invalid { error: "validate: …" }` (`:273-287`).
8. `store.insert(validated)`; `inserted` counts only
   `InsertOutcome::Inserted | InsertOutcome::Replaced` (`:288-291`).

**Skip vs abort, precisely:** a per-file *decode* failure, a per-file
*validation* failure, and a per-file *read* failure are all **skipped and
counted** — the run continues and previously inserted valid records are never
erased. Only two conditions **abort** the run: a prepare/scan seam failure
(`CacheLoaderError::Prepare`) and a scan-budget exhaustion
(`CacheLoaderError::ScanBudget`). All counting uses `saturating_add` /
`checked_add` (`:107`, `:117`, `:258`).

### 2. `reseed_ingest` — bounded, offline SU3 ingestion

Two entry points, both taking already-fetched bytes. Neither opens a socket,
accepts plain HTTP bytes, or reads a clock.

- `ingest_su3_into` (`:173`) → `i2pr_netdb::verify_su3_with_signers` (SU3
  framing via `i2pr_su3::parse`, signature via
  `i2pr_su3::verify_rsa_sha512` against `i2pr_su3::rsa_signer_from_certificate`).
  `ReseedVerifyOutcome::RejectedTrust { .. }` becomes
  `ReseedIngestError::UnknownSigner`; a verifier error becomes
  `Verification(String)`; zero accepted entries becomes `EmptyResult`
  (`:188-197`).
- `ingest_verified_archive_into` (`:221`) → `i2pr_netdb::verify_su3_archive`
  for bytes that already arrived through a locally-controlled offline path.
  Same `EmptyResult` rule, no signer lookup.

Verification actually performed: SU3 envelope framing and RSA-SHA512
(signature type 6, RSA over SHA-512 with a 4096-bit modulus — the reseed
protocol's only signature type) against an explicit, caller-configured trust
set, plus the DER X.509 signer-certificate checks (`RsaSha512_4096`,
signer-id length bound, NUL-free UTF-8, RSA-only key type, cert validity
interval) and per-entry filename/hash, decode, and Plan 103 validation. All of
that lives in `i2pr-netdb` and `i2pr-su3`; this crate only dispatches and
counts.

**What is actually bounded** (the `Default` ceilings, inherited from
`i2pr_netdb::ReseedLimits::default()` — verified values, not invented):

| Bound | Value | Owner |
| --- | --- | --- |
| `max_su3_bytes` | `8 * 1024 * 1024` (8 MiB) | `i2pr-netdb` `MAX_SU3_BYTES` |
| `max_archive_entries` | `4096` | `MAX_ARCHIVE_ENTRIES` |
| `max_archive_uncompressed_bytes` | `64 * 1024 * 1024` (64 MiB) | `MAX_ARCHIVE_UNCOMPRESSED_BYTES` |
| `max_archive_router_info_bytes` | `32 * 1024 * 1024` (32 MiB) | `MAX_ARCHIVE_ROUTERINFO_BYTES` |
| `max_entry_uncompressed_bytes` | `256 * 1024` (256 KiB) | `MAX_ENTRY_UNCOMPRESSED_BYTES` |
| `max_router_info_encoded_bytes` | `1024 * 1024` (1 MiB) | pinned to `i2pr_proto::MAX_COMMON_STRUCTURE_SIZE` by `From<ReseedIngestLimits> for ReseedLimits` (not caller-tunable) |

So iteration is bounded by entry count, expansion is bounded by cumulative and
per-entry uncompressed bytes, and every individual RouterInfo decode is capped
at 1 MiB. **Time is caller-supplied, not crate-bounded:** the `now_seconds`
argument is forwarded to `i2pr-netdb`, which uses it only to check the trusted
signer's certificate `not_before..=not_after` window
(`crates/i2pr-netdb/src/reseed.rs:824`, `:868`). This crate holds no clock and
starts no work of its own. The optional cache mirror is the last step
(`:199-211`, `:234-246`) and reuses `CacheLoader::cache()`'s `ByteCache`, so
the in-memory store and the on-disk cache stay in lock-step; the mirror is
opt-in via `cache: Option<&CacheLoader>`.

### 3. `floodfill_records` — versioned/checksummed durable envelope (M12)

The module doc is explicit (`floodfill_records.rs:3-5`): *"The envelope is
integrity-framed, not an authority token: every payload must be decoded and
cryptographically revalidated by the caller. Restored provenance is always
narrowed to a replica by consumers; reply secrets and source peer identifiers
are never serialized."*

**Envelope layout** — `MAGIC = b"I2FF"`, `HEADER_LEN = 4+1+1+1+1+1+8+4+32 = 53`,
`CHECKSUM_LEN = 32` (`:17-20`). Big-endian throughout; no padding, no
alignment slack.

| Offset | Len | Field | Bound on decode |
| --- | --- | --- | --- |
| `0` | 4 | magic `I2FF` | must equal `MAGIC`, else `InvalidEnvelope` |
| `4` | 1 | envelope version | must equal `FLOODFILL_ENVELOPE_VERSION` (= 1), else `InvalidEnvelope` |
| `5` | 1 | record type | `matches!(0 \| 1 \| 3 \| 7)` only; otherwise `UnsupportedType` |
| `6` | 1 | namespace | must be `0` — `NetDbNamespace::MainRouter` only |
| `7` | 1 | ingress | `0` DirectPeer, `1` RouterTunnel, `2` Local; else `InvalidEnvelope` |
| `8` | 1 | purpose | `0` PublishedStore, `1` FloodReplica, `2` LocalPublication; else `InvalidEnvelope` |
| `9` | 8 (`9..17`) | `observed_at_ms: u64` BE | exact `try_into` |
| `17` | 4 (`17..21`) | payload length `u32` BE | exact `try_into`, then `checked_add` |
| `21` | 32 | record key `Hash` | must equal the caller's `key`, else `InvalidEnvelope` |
| `53` | `length` | canonical payload bytes | exact-consumption: `HEADER_LEN + length + 32 == bytes.len()` |
| `53+length` | 32 | `SHA-256(bytes[..53+length])` | checksum mismatch → `InvalidEnvelope` |

`decode` also rejects the whole buffer when `len < HEADER_LEN + CHECKSUM_LEN`
or `len > max_bytes` (`:110-117`), and `encode` rejects out-of-floor record
types (`:78-80`), a payload that will not fit a `u32` (`:81-82`), and a total
longer than `max_bytes` (`:103-105`) — so a record that is too large is never
written and a previous committed file is preserved.

**The checksum is SHA-256** (`sha2::Sha256` over header+payload, `:102` and
`:143`) — an accidental-corruption detector only. It is *not* a signature and
carries no authority.

**Record-type floor: 0, 1, 3, 7 only.** Type 5 (EncryptedLeaseSet) is
rejected on both encode and decode. This matches the corrected M12 floor from
Plan 281 (`passed-m12-record-floor-corrected-type5-deferred`), which narrowed
ADR 0027 §4 to RouterInfo plus DatabaseStore types 1, 3, and 7 after Plan 280
stopped with no acceptable maintained I2P Red25519 (type 11) provider.

> **CRITICAL INVARIANT — restored provenance narrows to replica.**
> `load_validated_into` (`:256`) hard-codes the provenance it inserts with and
> never reads `envelope.purpose` back as authority (`:279-284`):
>
> ```rust
> let provenance = RecordProvenance {
>     namespace: NetDbNamespace::MainRouter,
>     inbound: InboundProvenance::RouterTunnel,
>     purpose: StorePurpose::FloodReplica,
>     observed_at_ms: now_ms,
> };
> ```
>
> The stored `PersistedPurpose` is provenance *metadata for the operator*, not
> an authority token. A record saved as `PublishedStore` or
> `LocalPublication` comes back as `FloodReplica`, and
> `ProvenanceIndex::may_replicate` maps `StorePurpose::FloodReplica` to
> `Eligibility::ReplicaCannotReflood`
> (`crates/i2pr-netdb/src/provenance.rs:269-270`) — a restored record can
> answer lookups but can never be re-flooded, and can never be upgraded to
> publisher authority by a restart. Only
> `PublishedStore` + `AuthenticatedDirectPeer` yields `Eligibility::Allowed`
> (`provenance.rs:275-278`).

**Every restored payload is decoded and revalidated** — the envelope check is
necessary but not sufficient. `load_validated_into` requires a caller
validation closure returning a typed `ValidatedNetDbRecord`
(`F: FnOnce(&FloodfillRecordEnvelope) -> Result<ValidatedNetDbRecord, ()>`,
`:266`) and then independently re-checks that the closure's output
`RecordId` type and key match both `envelope.record_type` and the requested
key (`:275-278`) — a closure that validates the wrong record is rejected with
`InvalidRecord`. It also applies an observed-age policy with saturating
arithmetic before validating (`:271-273`, `Expired`).

`load_router_info_into` (`:225`) is the typed RouterInfo specialisation: it
requires `record_type == 0`, runs
`i2pr_proto::RouterInfo::decode(&envelope.canonical_bytes,
i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)`, and then
`ValidatedRouterInfo::from_router_info(router_info, Some(RouterHash::from_hash(key)), context)`
— i.e. canonical decode plus full signature/freshness validation on every
restart, never a blind deserialize.

Capacity is enforced rather than silently trimmed: `ServerInsertOutcome::CapacityExceeded`
maps to `PersistError::Capacity` (`:286`). Reply keys/tags and source peer IDs
are never serialized, and the envelope's hand-written `Debug` redacts both the
key and the payload (`:46-56`).

**Durability.** `FloodfillRecordStore::save` goes through
`ByteCache::replace`, which writes and syncs a same-directory temporary under
`netdb/routers/.pending`, atomically renames it, then syncs the pending and
root directories; a failed pre-rename write preserves the previously committed
file. The root `scan()` skips non-files and rejects names that fail
`validate_name`, so `.pending` temporaries are never seen by the root scan and
never become authoritative. The pre-existing Plan 104 raw RouterInfo cache
remains readable through its original `cache_loader` path — the two formats
are independent and neither migrates the other.

**No production caller.** `FloodfillRecordStore` has no caller in
`i2pr-daemon` or `i2pr-runtime`; that is by design per Plan 282/283 ("nothing
in this plan ships a production floodfill path — the owner and the controlled
composition have no production caller by design").

## Dependencies

`crates/i2pr-netdb-persist/Cargo.toml` — production:

| Dependency | Source | What this crate uses it for |
| --- | --- | --- |
| `i2pr-crypto` | path | **Test-only in practice** — the only references are `i2pr_crypto::RouterIdentityBundle` in the `floodfill_records` test helper (`:399`, `:406`). It is declared in `[dependencies]`, so it sits in the allowlist, but no production path calls it |
| `i2pr-netdb` | path | `RouterInfoStore`, `ValidatedRouterInfo`, `ValidationContext`, `InsertOutcome`, `RouterHash`, `verify_su3_archive`, `verify_su3_with_signers`, `ReseedLimits`, `ReseedSignerTrustSet`, `ReseedVerifyReport`/`Outcome`, `ReseedEntryState`, `ReseedEntryReport`, `ServerNetDb`, `ServerInsertOutcome`, `ValidatedNetDbRecord`, `RecordProvenance`, `NetDbNamespace`, `InboundProvenance`, `StorePurpose`, `RecordId` |
| `i2pr-proto` | path | `RouterInfo` decode/encode, `Hash`, `Date`, `MAX_COMMON_STRUCTURE_SIZE` |
| `i2pr-storage` | path | `cache_seam::ByteCache`, `cache_seam::CacheError`, `MAX_CACHE_SCAN_ENTRIES`, `MAX_CACHE_SCAN_BYTES` |
| `thiserror` | workspace | `CacheLoaderError`, `ReseedIngestError`, `PersistError` derives |
| `sha2` | workspace | `Sha256` for the envelope checksum (`floodfill_records.rs:8`, `:102`, `:143`) |

Dev-dependencies: `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, `i2pr-storage`
(re-declared for direct use in the test modules), plus `rand_chacha`
(deterministic `ChaCha8Rng` identity generation), `rand_core`
(`SeedableRng`), and `tempfile` (cache fixture directories).

Checker allowlist — `scripts/check-dependency-direction.sh:31-33`:

```python
"i2pr-netdb-persist": {
    "i2pr-crypto", "i2pr-netdb", "i2pr-proto", "i2pr-storage"
},
```

`sha2` and `thiserror` are external crates and are not part of the
workspace-crate allowlist. No production crate depends on `i2pr-testkit`.

**Layering, stated unambiguously:**

```
[i2pr-storage]  bytes + ceilings + atomic replacement + dir permissions
      |  ByteCache::read / write / replace / scan / prepare
      v
[i2pr-netdb-persist]   <-- this crate: orchestration, budgets, typed reports,
      |                   envelope framing, provenance narrowing
      |  ValidatedRouterInfo / ValidatedNetDbRecord / ServerNetDb::insert
      v
[i2pr-netdb]     validation, store, provenance policy, reseed verifier
      ^
      |  i2pr_su3::parse / verify_rsa_sha512
[i2pr-su3]       SU3 envelope framing + RSA-SHA512 math
```

`i2pr-netdb` does **not** depend on `i2pr-storage`; the byte seam is supplied
from above, by this crate. `i2pr-daemon` is the only production consumer of
this crate's `CacheLoader` / `ReseedIngestor` entry points
(`crates/i2pr-daemon/src/bootstrap.rs`, `crates/i2pr-daemon/src/netdb_seam.rs`,
Plan 106).

### Filesystem and runtime-boundary posture

Stated precisely, because the honest answer is neither "pure" nor "I/O-owning":

- **This crate's own production source performs no direct filesystem call.**
  It holds no `std::fs`, `File`, `OpenOptions`, `std::net`, or `tokio` use in
  any non-test path; every byte read and write goes through the
  `i2pr_storage::cache_seam::ByteCache` methods `exists`, `prepare`, `scan`,
  `read`, `write`, and `replace`.
- **`std::fs` does appear in this crate, but only inside `#[cfg(test)]`
  modules** — `cache_loader.rs:335`, `:339` and `floodfill_records.rs:354`,
  `:441` use `std::fs::create_dir_all` / `set_permissions` to stage
  `tempfile` fixture directories. No production path is affected.
- **This crate is not covered by the runtime-neutral `std::fs` ban.**
  `scripts/check-runtime-boundaries.sh` applies that ban to
  `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` (line 44)
  and `i2pr-i2pcontrol` (line 53) — not to `i2pr-netdb-persist`, which is a
  composition layer whose legitimate job is to reach the byte cache.
- Confirmed absent from the whole crate: no `tokio`, no `std::net`, no
  `TcpStream`/`TcpListener`/`UdpSocket`, no `async fn`, no `spawn`, no
  unbounded channel, no clock, and `#![forbid(unsafe_code)]` (`lib.rs:23`).
  Grep-verified across all four files.
- What it inherits transitively is the storage layer's own I/O — that is the
  intended seam, not a purity claim about the stack.

## Tests

There is **no `crates/i2pr-netdb-persist/tests/` directory** and no fixture
directory owned by this crate. All coverage is three in-crate
`#[cfg(test)] mod tests` blocks; `cargo test --locked -p i2pr-netdb-persist`
reports **9 passed, 0 failed, 0 ignored**, plus 0 doc-tests.

| Test | Source | What it proves |
| --- | --- | --- |
| `missing_cache_is_empty_report` | `cache_loader.rs:347` | An absent cache directory is a valid bootstrap state: `entries_inspected == 0`, store stays empty, no error |
| `cache_loader_rejects_unknown_filename` | `cache_loader.rs:361` | A syntactically valid 64-hex filename holding garbage bytes yields `unreadable + invalid == 1` — the run **isolates** the bad entry and still returns `Ok`, proving the filename↔identity key binding rejects it |
| `ingest_without_signers_rejects_unknown_signer` | `reseed_ingest.rs:294` | Empty bytes with an empty trust set produce a typed failure (`Verification` or `UnknownSigner`); the test deliberately asserts the disjunction because header parsing fails before the trust lookup |
| `from_report_to_summary_counts_outcomes` | `reseed_ingest.rs:316` | `ReseedSummary::from` folds `ReseedEntryState` into `total`/`accepted`/`rejected_filename` |
| `cache_loader_report_field_is_accessible` | `reseed_ingest.rs:343` | `CacheLoaderReport::record` returns `None` for a missing name |
| `envelope_round_trips_and_rejects_corruption_version_and_truncation` | `floodfill_records.rs:309` | Round-trip equality; the `Debug` output does **not** contain the payload bytes; one-byte truncation fails; a single flipped payload bit fails the SHA-256 check; an unknown version byte yields `InvalidEnvelope` |
| `envelope_format_covers_only_the_adr_supported_type_floor` | `floodfill_records.rs:333` | Types 0/1/3/7 round-trip; type 5 yields `UnsupportedType(5)` on encode |
| `cache_replacement_preserves_complete_latest_envelope` | `floodfill_records.rs:352` | `ByteCache::replace` leaves only the *complete* newest envelope after two writes; an oversized write is refused with `TooLarge` and the previous committed file survives |
| `router_info_restart_revalidates_and_narrows_provenance` | `floodfill_records.rs:430` | The security invariant, end to end (Plan 282 §10 row 10): a real signed RouterInfo saved as `PersistedPurpose::PublishedStore` and restored yields `ServerInsertOutcome::Inserted`, `may_replicate` returns `ProvenanceEligibility::ReplicaCannotReflood`, and the record still answers lookups. Then: tampered payload bytes → `InvalidRecord` (never enters the NetDB); a stale observation → `Expired` |

**Honest coverage gaps.** The three `reseed_ingest` tests never exercise a
successful SU3 verification, a real signature, or the cache-mirror branch —
that path is covered downstream in `i2pr-netdb`'s reseed suite and
`i2pr-daemon`'s bootstrap tests. `floodfill_records` has test coverage for
record type 0 only; types 1, 3, and 7 are proven to round-trip and to be
accepted by the floor check, but there is no type-specific
`load_validated_into` closure test for them. `CacheLoaderScanBudget` has no
direct unit test for capacity-1 / exact-load / max+1; its bounds are exercised
only indirectly, and no test drives `load_into_with_limits` to a
`ScanBudget` abort.

## Distinctive design choices

1. **Composition only, by explicit contract** — the crate doc names the four
   things it must not embed (validation, hashing, RouterInfo decoding,
   filesystem policies) and the module layout follows: every one of those is
   a call into `i2pr-netdb`, `i2pr-su3`, or `i2pr-storage`.
2. **Integrity framing is not an authority token** — the `I2FF` SHA-256
   checksum detects corruption only; authority comes from re-running full
   decode + signature + freshness validation on every restore.
3. **Stored provenance is metadata; restored provenance is policy** —
   `load_validated_into` hard-codes `RouterTunnel` + `FloodReplica` and
   discards the stored `PersistedPurpose`, so a restart can never widen
   authority.
4. **Key binding is re-established twice** — the filename-derived key in
   `cache_loader` and the caller's expected key in `floodfill_records::decode`
   are both checked against the record's own identity/hash, and
   `load_validated_into` re-checks the closure's `RecordId` a third time.
5. **Skip-vs-abort is asymmetric on purpose** — a corrupt record is skipped
   and counted so one bad file cannot poison a corpus, but a scan-budget
   overrun aborts the run rather than silently truncating it.
6. **Redaction is hand-written where bytes or identities are involved** —
   `FloodfillRecordEnvelope` implements `Debug` itself and prints
   `key`/`canonical_bytes` as `[redacted]` rather than deriving it.
7. **Capacity and age are typed refusals, not silent trims** —
   `PersistError::Capacity` and `PersistError::Expired` surface instead of a
   partial restore.
8. **`ReseedIngestor<'a>` borrows only the trust set, never the store** — the
   target `RouterInfoStore` is a per-call parameter, so one ingestor can serve
   many stores without the composition outliving anything.
9. **Limits are inherited, not restated** — the reseed defaults read from
   `i2pr_netdb::ReseedLimits::default()` and the cache defaults from
   `i2pr_storage::cache_seam::MAX_CACHE_SCAN_*`, so there is exactly one place
   to change each ceiling.
10. **Two independent on-disk formats coexist** — the Plan 104 raw RouterInfo
    cache and the Plan 276 `I2FF` envelope share a `ByteCache` root but
    neither reads or migrates the other.

## Cross-references

- [Overview](overview.md) — crate index and data flow.
- [Dependency graph](dependency-graph.md) — the workspace-crate allowlist this
  crate's entry mirrors.
- [i2pr-netdb](i2pr-netdb.md) — owns the validator, the stores, the provenance
  policy (`ProvenanceIndex::may_replicate`), the server NetDB, and the reseed
  verifier this crate dispatches into.
- [i2pr-storage](i2pr-storage.md) — owns `cache_seam::ByteCache`, the scan
  ceilings, the `.pending` staging directory, and the `0o700` policy.
- [i2pr-su3](i2pr-su3.md) — owns SU3 envelope framing and the RSA-SHA512
  (signature type 6) verification math.
- [i2pr-proto](i2pr-proto.md) — owns the `RouterInfo` codec and
  `MAX_COMMON_STRUCTURE_SIZE`.
- [i2pr-daemon](i2pr-daemon.md) — the only production consumer
  (`bootstrap.rs`, `netdb_seam.rs`).
- ADR 0027 — `docs/adr/0027-floodfill-role-provenance-and-advertisement.md`:
  role boundaries, provenance, persistence, and the advertisement gate whose
  §4 record floor was corrected by Plan 281.
- Closure records (authoritative over `plans/registry.md`):
  - `plans/closure/netdb/103-status.md` — the Plan 103 validator and store.
  - `plans/closure/netdb/104-status.md` — this crate's originating plan
    (cache seam + reseed trust path).
  - `plans/closure/netdb/105-status.md` — transport-neutral NetDB query state
    machines that consume these typed APIs.
  - `plans/closure/netdb/106-status.md` — daemon/bootstrap integration.
  - `plans/closure/floodfill/276-status.md` — passed; the versioned bounded
    envelope, mandatory revalidation, and the "restored authority cannot
    expand" rule.
  - `plans/closure/floodfill/280-status.md` — stopped; no acceptable
    maintained I2P Red25519 (type 11) provider.
  - `plans/closure/floodfill/281-status.md` — passed; record floor corrected
    to 0/1/3/7 with type 5 deferred.
  - `plans/closure/floodfill/282-status.md` — stopped at the above-floor
    reachability evidence boundary; retained the revalidate-and-narrow work
    proven by the row-10 test.
  - `plans/closure/floodfill/283-status.md` — passed with third-class evidence
    and controlled activation.
- **Unimplemented and unclaimed:** the daemon floodfill **role lifecycle**
  (Plan 277 stopped at the daemon/runtime integration boundary) and **all**
  floodfill advertisement remain unimplemented; no `caps=f` role or
  advertisement is claimed or activated.
