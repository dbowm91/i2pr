# Plan 276 status — passed persistence, maintenance, and resource governance

Status: **passed-m12-versioned-floodfill-persistence-maintenance-and-resource-governance**

Implementation commit: recorded in Git history for this closure.

## Requirement-to-evidence

| Requirement | Evidence |
|---|---|
| Versioned bounded envelope for supported types 0/1/3/7 | `FloodfillRecordEnvelope`, format version 1, explicit main namespace, type/key, provenance ingress/purpose, observation time, length-prefixed canonical bytes, SHA-256 corruption check; type 5 is rejected |
| Revalidation is mandatory on restart and key binding is checked | `FloodfillRecordStore::load_validated_into` requires a caller validation closure returning a typed `ValidatedNetDbRecord`, then checks its type/key before insertion; RouterInfo helper performs canonical decode/signature/freshness validation |
| Restored authority cannot expand | all loads insert as `RouterTunnel` + `FloodReplica`, ignoring stored publisher purpose; reply keys/tags and peer IDs are not encoded |
| Atomic replacement and recoverability | `ByteCache::replace` writes/syncs a same-directory temporary, atomically renames it, then syncs pending/root directories; failed pre-rename writes preserve the previous committed file; test verifies complete replacement and oversized-write preservation |
| Corrupt/truncated/unknown-version/type5 rejection and type envelope round-trip | `floodfill_records` unit tests for checksum corruption, truncation, version, key/type bounds, supported type set, and replacement |
| Bounded incremental maintenance and provenance cleanup | `ServerNetDb::maintenance_batch`, cursor over bounded provenance IDs, synchronous payload/provenance/byte-accounting removal; `daily_rollover_due` is caller-time and clock-regression safe |
| Floodfill resource ceilings and release behavior | `FloodfillResourcePolicy`, `FloodfillResourceBudget`, privacy-safe snapshots, RAII leases for request/effect/byte/crypto/flood/maintenance work; exact-capacity and drop-to-baseline test |
| Existing raw RI cache/reseed loader retained | existing loader and reseed APIs unchanged; Plan 104 format remains readable by its loader |
| Boundary checker remains fail-closed | `scripts/check-m12-floodfill-boundaries.sh`; its exact one-tag regex was corrected to match literal parentheses rather than an empty regex group |

## Verification

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` — 167 passed.
- `rtk cargo test --locked -p i2pr-netdb-persist --all-targets` — 8 passed.
- `rtk cargo test --locked -p i2pr-storage --all-targets` — 19 passed.
- `rtk cargo clippy --locked -p i2pr-netdb -p i2pr-netdb-persist -p i2pr-storage --all-targets -- -D warnings` — passed.
- `rtk bash scripts/check-dependency-direction.sh` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
- `rtk bash scripts/check-m12-floodfill-boundaries.sh` — passed.
- `rtk git diff --check` — passed.

## Security, operational limits, and findings

Durable floodfill payloads remain untrusted bytes. The envelope SHA-256 checksum is corruption
framing, not a signature or secret MAC; trust comes only from revalidation. Every restored record
is conservatively a non-refloodable replica. Persistence is a composition API and is not yet
attached to a daemon role. Maintenance is caller-driven and bounded by batch size. Resource leases
are runtime-neutral and add no task/socket ownership. No capability advertisement is enabled.
No dependency beyond the existing workspace SHA-256 implementation was added. No critical/high
findings remain open.

## Unblock audit and roadmap disposition

Plan 277's hard dependency is satisfied. Move Plan 277 to ready. Plans 278–279 remain blocked in
sequence pending daemon lifecycle and qualification. Plan 280 remains stopped due to provider
unavailability; Plan 281's type-5 deferral remains authoritative.
