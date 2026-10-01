# Plan 276 — M12 persistence, maintenance, and floodfill resource-governance closure

Status at registration:
**passed-m12-versioned-floodfill-persistence-maintenance-and-resource-governance**

Classification: infrastructure/security capability.

Hard dependency: Plan 275 passed.

## 1. Objective

Complete the durable and long-running floodfill data-plane prerequisites: expiry/maintenance,
versioned persistence where justified, restart revalidation with provenance preservation, storage
pressure behavior, and explicit resource budgets across store/lookup/replication work.

## 2. Invariants

- Persisted NetDB data is untrusted and revalidated on every load.
- Restart never upgrades disclosure/replication authority.
- Partial/corrupt persistence cannot poison the in-memory authoritative store.
- Short-lived record classes need not be persisted merely to satisfy M12; the ADR decides the
  persistence set. RouterInfos and ADR-selected long-lived Meta/Encrypted records are the expected
  durable candidates.
- Count and bytes are bounded per type and globally.
- Expiry frees both record bytes and provenance metadata.
- Unexpired authoritative state is not silently evicted to admit lower-priority data unless the
  ADR/policy explicitly defines deterministic pressure eviction.
- Maintenance has bounded work per tick; no full-store unbounded scan on every request.
- Floodfill queues/work budgets are subordinate to router-wide resource governance.

## 3. Required production changes

A. Add maintenance scheduler inputs as pure incremental/tick APIs in i2pr-netdb: expiry cursor or
bounded batch, stale provenance cleanup, rollover maintenance trigger, and stats.

B. Extend i2pr-netdb-persist with a versioned record envelope containing type, namespace/provenance
class, canonical bytes, observation metadata required by ADR, and integrity framing. Do not store
secret reply keys/tags.

C. Atomic/recoverable write path using existing storage primitives; define fsync/rename behavior
consistent with repository storage policy.

D. Restart loader re-decodes/revalidates every record, rechecks freshness and key binding, and
re-derives answer/flood eligibility from persisted provenance plus current policy.

E. Add FloodfillResourcePolicy covering active request slots, queued effects, aggregate queued
bytes, crypto/decompression budget, direct-flood attempts, and maintenance work.

## 4. Scope / non-goals

No live Tokio service, transport send, config, RouterInfo capability, or external router tests.

## 5. Work packages

1. Freeze persistence format version and migration policy.
2. Implement bounded maintenance APIs.
3. Implement persist/load for ADR-selected classes.
4. Add corruption/truncation/partial-write/restart tests.
5. Add resource-accounting model and pressure tests.
6. Add privacy-safe aggregate snapshots for Plan 277 health evaluation.

## 6. Failure / cancellation / restart / contention

Persistence failures are typed and may degrade persistence without enabling unsafe serving.
Restart skips/quarantines invalid entries rather than trusting old metadata. Atomic write failure
must leave the previous committed snapshot usable.

Maintenance cancellation is represented by caller stopping between bounded batches; no half-applied
single-record mutation.

## 7. Compatibility and migration

New persistence format is internal/experimental and versioned. If an older Plan 104 RI cache
format remains, loader migration must be explicit and one-way or both formats must be safely
recognized. No user-facing compatibility promise is created.

## 8. Required tests

- clean persist/reload for each persisted type;
- key/signature/freshness revalidation on reload;
- provenance does not upgrade after restart;
- unknown version, truncated header/body, checksum/integrity corruption, duplicate key conflict;
- partial temp file and interrupted rename recovery;
- expired-on-restart removal;
- quota pressure and deterministic refusal/eviction;
- bounded maintenance batch;
- resource budget exhaustion/return-to-baseline;
- no secrets in persistence or Debug;
- existing reseed/cache loader regressions.

## 9. Exact verification

~~~bash
cargo fmt --all --check
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-netdb-persist --all-targets
cargo test --locked -p i2pr-storage --all-targets
cargo clippy --locked -p i2pr-netdb -p i2pr-netdb-persist -p i2pr-storage --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
~~~

## 10. Documentation

Document persistence format/version, what is intentionally non-persistent, corruption behavior,
resource ceilings, and restart semantics.

## 11. Acceptance criteria

- Restart revalidation is mandatory and tested.
- Provenance/namespace authority survives or conservatively narrows across restart.
- Maintenance/resource work is bounded.
- Corruption/partial writes fail safely.
- No runtime network activation/caps=f.
- No critical/high finding remains.

## 12. Stop conditions

Stop if persistence requires serializing secret/session state, if existing storage APIs cannot
provide recoverable writes, or if pressure handling would require silent eviction of authoritative
unexpired state.

## 13. Closure evidence

Persistence format table, restart/corruption matrix, budget measurements, exact commands,
dependency review, and unblock audit. On pass, move Plan 277 to ready.

## 14. Handoff

Plan 277 composes these pure services into supervised daemon effects and may use only the
privacy-safe aggregate health snapshot.
