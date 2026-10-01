# Plan 271 status — passed provenance, namespace, and disclosure foundation

- Plan: `plans/implementation/floodfill/271-m12-netdb-provenance-segmentation.md`
- Parent: Plan 270 close, commit `4638d0e`.
- Disposition: **passed**. The runtime-neutral NetDB crate now exposes a bounded provenance
  metadata index and typed namespace/disclosure policy. No serving, replication, persistence,
  wire, daemon, or capability-advertisement path was added.

## API migration and ownership

| Existing surface | Disposition |
|---|---|
| `RouterInfoStore::insert/get/iter` | Retained for current validated client NetDB behavior. No floodfill service exists; these APIs are not an answerable server view. |
| `LeaseSet2Store::insert/get/iter` | Retained for current Standard LS2 client lookup behavior under the same limitation. |
| `ProvenanceIndex` | New explicit metadata API keyed by redacted `(record_type, record key)` identity. All inserted metadata requires namespace, ingress, purpose, and observation time. |
| `eligible_records` and answer/replication/persistence methods | New namespace-filtered, time-bounded policy views. Callers must still obtain the corresponding validated record; there is no implicit fallback. |

The legacy stores remain existing client adapters; they were not migrated to arbitrary server
provenance because no server path consumes them. Plan 272 must integrate record replacement and
metadata updates atomically before making any record server eligible.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| Typed namespace, ingress, purpose, and observed time | `crates/i2pr-netdb/src/provenance.rs`: `NetDbNamespace`, opaque `ClientNamespaceId`, `InboundProvenance`, `StorePurpose`, `RecordProvenance`. |
| Bounded metadata independent of record bytes | `ProvenanceLimits` and `ProvenanceIndex`; count and accounted-byte failures reject without mutation. |
| Pure, typed answer/replicate/persist policy | `may_answer_router_lookup`, `may_answer_client_lookup`, `may_replicate`, `may_persist`, and `eligible_records` return `ProvenanceEligibility` reasons. |
| Lookup response and flood replica cannot be promoted | Unit test `isolates_namespaces_and_denies_response_and_replica_promotion`. |
| Count/byte quota and replacement atomicity; redacted debug | Unit test `quota_rejection_and_replacement_are_atomic_and_debug_is_redacted`. |
| Deterministic expiry | Unit test `time_policy_expires_deterministically`, with caller-supplied time. |
| Runtime/dependency boundary | Existing dependency allowlist and runtime boundary checks pass; `i2pr-netdb` remains runtime-neutral. |
| Client compatibility and no new serving behavior | `cargo test -p i2pr-netdb --all-targets` and daemon `netdb_integration` pass; no runtime/daemon sources changed. |

## Unblock audit

Plan 280 is the only newly ready plan: it must qualify a vetted I2P Red25519 type-11 verifier
for the Plan 272 type-5 validation requirement. Plan 272 remains blocked until both Plans 271
and 280 pass. Plans 273–279 remain blocked by the dependency chain. No other plan became eligible.

## Verification

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-netdb --all-targets` — 143 passed.
- `rtk cargo test --locked -p i2pr-daemon --test netdb_integration` — 38 passed.
- `rtk cargo clippy --locked -p i2pr-netdb --all-targets -- -D warnings` — passed.
- `rtk bash scripts/check-dependency-direction.sh` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
