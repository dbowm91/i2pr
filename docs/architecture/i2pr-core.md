# `i2pr-core` — Deep Dive

Runtime-neutral service contracts. The bottom of the runtime-aware
dependency graph and the only crate with **zero** direct dependencies.

Path: `crates/i2pr-core/`

## Purpose

Owns small, std-only types that the future router services all share:

- **Lifecycle** state machine and bounded service names.
- **Health** snapshots, bounded diagnostic detail, liveness/readiness,
  redaction.
- **Cancellation** via a runtime-neutral `Arc<AtomicBool>` token.
- **Resource budgets** with per-class ceilings, RAII leases, atomic
  bundles, high-water marks, denial counters, and unwind safety.
- **Failure classification** taxonomies for both consumer-reported
  (`ServiceFailure`) and supervisor-observed (`ServiceCompletion`)
  exits.

It deliberately owns **no** runtime, configuration parsing, filesystem
state, network transport, protocol codec, or router composition. It
depends on **nothing** — not even `tokio` or `i2pr-*` crates.

## Module layout

Single flat file: `crates/i2pr-core/src/lib.rs` (~1425 lines). All types
live at the crate root. There are no submodules.

## Public surface

### Constants

- `MAX_SERVICE_NAME_BYTES` (17), `MAX_HEALTH_DETAIL_BYTES` (19),
  `MAX_RESOURCE_CLASSES` (542).

### Service naming and lifecycle

- `struct ServiceName` (23) — bounded, validated UTF-8 (≤ 64 bytes).
  `AsRef<str>` only.
- `enum ServiceNameError` (60) — `Empty`, `TooLong { maximum }`.
- `enum LifecycleState` (82) — `Registered → WaitingForDependencies →
  Starting → Ready → Degraded → Stopping → Stopped/Failed`.
  Self-transitions accepted; illegal transitions return
  `InvalidLifecycleTransition`.
- `struct InvalidLifecycleTransition` (139) — error value with
  `from`/`to` fields.

### Failure taxonomy

- `enum ServiceClassification` (160) — `Essential / Restartable /
  Degradable / Optional`.
- `enum FailureCategory` (173) — 10-variant static taxonomy:
  `ServiceFailure`, `UnexpectedCleanExit`, `Panic`, `TaskJoinFailure`,
  `StartupTimeout`, `ReadinessTimeout`, `GracefulShutdownTimeout`,
  `ForcedAbort`, `RestartBudgetExhausted`, `DependencyUnavailable`.
- `enum ServiceFailureCategory` (198) — `Internal /
  DependencyUnavailable / ResourceExhausted / InvalidState`.
- `struct ServiceFailure` (211) — typed failure with bounded
  `HealthDetail`.
- `enum ServiceCompletion` (234) — 10-variant mirror of
  `FailureCategory` plus `RequestedShutdown`.
- `enum CancellationReason` (283) — bounded reasons: `OperatorRequest /
  EssentialServiceFailure / StartupFailure / ShutdownDeadline /
  ParentScope / TestHarnessTeardown`.
- `enum DegradationCode` (300) — `DependencyUnavailable /
  ResourcePressure / LocalPolicy`.

### Health

- `enum HealthState` (311) — `Starting / Ready / Degraded(code) /
  Stopping / Failed`.
- `struct HealthDetail` (338) — bounded diagnostic string (≤ 160
  bytes). `Debug` impl **redacts** content (`{ redacted: true }`).
- `enum HealthDetailError` (369) — `TooLong { maximum }`.
- `struct HealthSnapshot` (385) — immutable observation with
  liveness/readiness flags, restart count, monotonic sequence, timing.

### Shutdown / cancellation

- `enum ShutdownReason` (512) — `Requested / Signal / FatalFailure /
  Configuration / Test`.
- `struct CancellationToken` (527) — runtime-neutral
  `Arc<AtomicBool>` with `cancel` / `is_cancelled` / `clone` (shared).

### Resource governor

- `enum ResourceClass` (546) — 17 categories: `ServiceTasks,
  ChildTasks, CommandQueueItems, EventQueueItems, BufferedBytes,
  SimulatedStreamLinks, SimulatedDatagramLinks, PendingTimers,
  TestPeers, Tasks, PendingHandshakes, ActiveLinks, NetDbQueries,
  TunnelBuilds, Destinations, Streams, ApiSessions`.
- `ResourceClass::ALL` (585) and `::COUNT` (606).
- `struct ResourceLimit` (611), `ResourceRequest` (631),
  `ResourceUsage` (651), `ResourceBudget` (705), `ResourceBundle` (973),
  `ResourceLease` (1001).
- `enum ResourceError` (1034) — 10 variants: `ZeroLimit, ZeroRequest,
  DuplicateLimit, DuplicateRequest, EmptyBundle, TooManyClasses,
  MissingLimit, Exhausted, ArithmeticOverflow, Poisoned`.

## Key contracts

The crate defines **zero traits**. All contracts are concrete
structs/enums with methods.

- **Lifecycle FSM**: `LifecycleState::transition(self, next)` enforces
  a hardcoded state graph, returns `InvalidLifecycleTransition` on
  illegal moves.
- **Health reporting**: `HealthSnapshot` (385) is an immutable value
  object. Liveness and readiness are derived from `HealthState`.
- **Cancellation**: `CancellationToken` is just `Arc<AtomicBool>` —
  no waker, no async, no `select!` integration. `i2pr-runtime`
  wraps this with `tokio_util::CancellationToken`.
- **Resource budgets**:
  - `ResourceBudget` is `Mutex`-guarded (not `RwLock`).
  - `try_acquire` grants single-class leases; `try_acquire_bundle`
    grants multiple classes atomically (sorted by class, all-or-nothing).
  - `ResourceLease` releases on drop (RAII).
  - Tracks `used` / `high_water` / `denied` / `release_underflow`
    per class.
  - `release()` is panic-safe (uses `into_inner()` on a poisoned
    mutex).

## Errors

`ServiceNameError`, `HealthDetailError`, `InvalidLifecycleTransition`,
`ResourceError`. All implement `Display + Error`.

`HealthDetail::Debug` redacts: it prints `{ redacted: true }`,
preventing diagnostic strings from leaking into logs.

## Dependencies

**Zero direct dependencies.** `Cargo.toml` declares no
`[dependencies]` block. Only `std` is used:

- `std::sync::atomic::AtomicBool`
- `std::sync::{Arc, Mutex}`
- `std::collections::BTreeMap`
- `std::time::Duration`
- `std::borrow::Borrow`, `std::fmt`

Confirmed: no `tokio`, no `std::net`/`std::fs`, no dependency on any
other workspace crate. It is the leaf of the production dependency
graph and is depended on by `i2pr-transport`, `i2pr-runtime`,
`i2pr-daemon`, and `i2pr-testkit`.

## Tests

Inline in `src/lib.rs:1106-1425` — 13 synchronous `#[test]` functions:

| Test | Line | Coverage |
| --- | --- | --- |
| `lifecycle_rejects_recovery_from_stopped` | 1114 | FSM rejection |
| `health_snapshot_exposes_typed_readiness` | 1129 | Snapshot liveness/readiness |
| `resource_lease_releases_on_drop_and_rejects_overcommit` | 1142 | RAII + overcommit |
| `resource_classes_and_snapshots_are_bounded_and_deterministic` | 1154 | Class count + ordering |
| `resource_usage_records_exact_limit_denial_and_high_water` | 1182 | High-water + denial counting |
| `resource_validation_rejects_zero_and_handles_u64_overflow` | 1212 | Zero + overflow |
| `resource_release_is_consuming_drop_safe_and_unwind_safe` | 1249 | Panic-unwind safety |
| `invalid_release_is_visible_without_wrapping_or_panicking` | 1278 | Underflow counter |
| `resource_bundle_is_atomic_sorted_and_releases_together` | 1294 | Bundle atomicity |
| `resource_bundle_rejects_duplicates_without_mutation` | 1339 | Duplicate/empty rejection |
| `concurrent_acquisition_never_exceeds_the_limit` | 1359 | 16-thread race |
| `bounded_types_reject_oversized_values` | 1405 | Name/detail max bytes |
| `health_detail_debug_is_redacted` | 1411 | Redaction check |
| `cancellation_is_shared_by_clones` | 1419 | Clones share the flag |

## Distinctive design choices

- **Zero dependencies** — uncommon and deliberate. The crate pulls
  only `std` primitives.
- **No traits at all** — every contract is concrete. Runtime code
  uses these concrete types directly. The doc comments suggest this
  is intentional ("runtime-neutral contracts shared by the future
  router services").
- **`HealthDetail::Debug` redacted** — privacy/security measure.
- **`CancellationToken` is intentionally minimal** — just
  `Arc<AtomicBool>`. The runtime layer owns the wakeable variant.
- **Bundle atomicity is enforced by sorting + pre-validation** —
  prevents deadlocks from inconsistent ordering without runtime
  coordination.
- **Panic-safe `release`** — recovered via `into_inner()` on a
  poisoned mutex.
- **`ServiceName` only implements `AsRef<str>`** — no `AsRef<[u8]>`.

## Cross-references

- [Overview](overview.md)
- [i2pr-runtime](i2pr-runtime.md) — adds wakeable cancellation and
  uses `LifecycleState`, `ServiceClassification`, `HealthSnapshot`,
  `HealthDetail`, `ResourceBudget`, `ShutdownReason`,
  `CancellationReason`, `DegradationCode`, `FailureCategory`,
  `InvalidLifecycleTransition`, `ServiceCompletion`, `ServiceFailure*`,
  `ServiceName`.
- [i2pr-transport](i2pr-transport.md) — re-exports the resource
  budget types at its crate root for runtime services.
- [i2pr-testkit](i2pr-testkit.md) — uses `ResourceBudget` to track
  pending timers, buffered bytes, and link leases.
- Plan-of-record: series of milestone closures under `plans/0*`.
