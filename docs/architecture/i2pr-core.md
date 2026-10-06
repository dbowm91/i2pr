# `i2pr-core` — Deep Dive

Runtime-neutral service contracts: lifecycle, health, cancellation, and
the shared resource governor. The bottom of the runtime-aware
dependency graph and the only crate with **zero** dependencies.

Path: `crates/i2pr-core/`

## Purpose

Owns the small, `std`-only types every other crate shares:

- **Lifecycle** state machine, terminal-state query, and bounded
  service names.
- **Health** snapshots with explicit liveness/readiness, bounded
  diagnostic detail, and redacted `Debug`.
- **Cancellation** via a runtime-neutral, polling-only
  `Arc<AtomicBool>` token.
- **Resource governance** — per-class ceilings, RAII leases, atomic
  bundles, high-water marks, denial counters, and unwind safety.
- **Failure and completion taxonomies** for both consumer-reported
  (`ServiceFailure`) and supervisor-observed (`ServiceCompletion`)
  exits.

It deliberately owns **no** runtime, configuration parsing,
filesystem state, network transport, protocol codec, or router
composition. It depends on **nothing** — not even `tokio` or another
`i2pr-*` crate.

## Module layout

One flat file, no submodules, no subdirectories. All public items live
at the crate root.

| File | Lines | Notes |
| --- | --- | --- |
| `crates/i2pr-core/src/lib.rs` | 1425 | `#![forbid(unsafe_code)]` at :7; `#[cfg(test)] mod tests` at :1105-1425 |

Internal concept breakdown (not modules — declaration ranges in
`lib.rs`):

| Concept | Lines |
| --- | --- |
| Crate docs, `forbid(unsafe_code)`, `std` imports | 1-14 |
| Bound constants | 16-19 |
| Service naming | 21-78 |
| Lifecycle FSM | 80-156 |
| Failure / completion taxonomy | 158-279 |
| Cancellation reasons, degradation codes | 281-307 |
| Health state, detail, snapshot | 309-508 |
| Shutdown reason, cancellation token | 510-539 |
| Resource governor (classes → errors) | 541-1103 |
| In-crate tests | 1105-1425 |

Private helpers: `ClassState`, `BudgetState`, `BudgetInner` (:684-701)
and `ResourceBudget::release` / `release_for_test` (:946-968).

## Public surface

28 public items: 3 constants and 25 types. No traits are exported.

### Constants

- `MAX_SERVICE_NAME_BYTES: usize = 64` (lib.rs:17) — service
  identifier ceiling.
- `MAX_HEALTH_DETAIL_BYTES: usize = 160` (lib.rs:19) — bounded health
  context ceiling.
- `MAX_RESOURCE_CLASSES: usize = 32` (lib.rs:542) — class-count
  ceiling enforced by `ResourceBudget::new` and
  `try_acquire_bundle`. Enforced by a `const` assertion against
  `ResourceClass::COUNT` in the test suite.

### Service naming and lifecycle

- `struct ServiceName` (23) — bounded (≤ `MAX_SERVICE_NAME_BYTES`)
  non-empty UTF-8 newtype. `new`, `as_str`, `AsRef<str>`, and
  `Display`; `Clone + Debug + Eq + Hash + Ord`.
- `enum ServiceNameError` (60) — `Empty`, `TooLong { maximum: usize }`.
- `enum LifecycleState` (82) — 8 variants: `Registered`,
  `WaitingForDependencies`, `Starting`, `Ready`, `Degraded`,
  `Stopping`, `Stopped`, `Failed`. Methods: `transition(self, next) ->
  Result<Self, InvalidLifecycleTransition>`, `is_terminal(self) ->
  bool` (`Stopped | Failed`).
- `struct InvalidLifecycleTransition` (139) — public `from` /
  `to: LifecycleState` fields.

### Failure and completion taxonomy

- `enum ServiceClassification` (160) — `Essential / Restartable /
  Degradable / Optional`.
- `enum StartupRequirement` — `Required` (the `#[default]`) / `Optional`.
  Deliberately **not** a fifth `ServiceClassification`: that enum describes how
  a service behaves once the router is running, while this one describes
  whether the router needs it ready before startup completes. The two are
  orthogonal. `gates_readiness()` is the predicate the runtime snapshot uses.
- `enum FailureCategory` (173) — 10-variant static taxonomy:
  `ServiceFailure`, `UnexpectedCleanExit`, `Panic`,
  `TaskJoinFailure`, `StartupTimeout`, `ReadinessTimeout`,
  `GracefulShutdownTimeout`, `ForcedAbort`,
  `RestartBudgetExhausted`, `DependencyUnavailable`.
- `enum ServiceFailureCategory` (198) — `Internal /
  DependencyUnavailable / ResourceExhausted / InvalidState`.
- `struct ServiceFailure` (211) — private `category` plus optional
  bounded `HealthDetail`. `new` (const), `category` (const),
  `detail`.
- `enum ServiceCompletion` (235) — 10 variants: `RequestedShutdown`,
  `UnexpectedCleanExit`, `Failed(ServiceFailure)`, `Panic`,
  `TaskJoinFailure`, `StartupTimeout`, `ReadinessTimeout`,
  `GracefulShutdownTimeout`, `ForcedAbort`,
  `RestartBudgetExhausted`. It mirrors 9 of the 10
  `FailureCategory` values (`DependencyUnavailable` is not
  mirrored) and carries the typed failure as a payload instead of
  a bare `ServiceFailure` marker. Methods: `category() ->
  Option<FailureCategory>` (`None` only for `RequestedShutdown`),
  `is_failure() -> bool`.
- `enum CancellationReason` (283) — `OperatorRequest /
  EssentialServiceFailure / StartupFailure / ShutdownDeadline /
  ParentScope / TestHarnessTeardown`.
- `enum DegradationCode` (300) — `DependencyUnavailable /
  ResourcePressure / LocalPolicy`.

### Health

- `enum HealthState` (311) — `Starting / Ready / Degraded(code) /
  Stopping / Failed`. Methods: `is_live(self) -> bool` (false only
  for `Stopping | Failed`), `is_ready(self) -> bool` (`Ready` only).
- `struct HealthDetail` (338) — bounded (≤ `MAX_HEALTH_DETAIL_BYTES`)
  diagnostic string. `new`, `as_str`. Its **hand-written `Debug`** (:340)
  prints `HealthDetail { redacted: true }` and never the content.
- `enum HealthDetailError` (369) — `TooLong { maximum: usize }`.
- `struct HealthSnapshot` (385) — all fields private. Two
  constructors: `new(state, transition_sequence, detail)` (:399)
  derives the matching `LifecycleState` and leaves
  service/classification/restart/last-failure unset with
  `transition_time = Duration::ZERO`; `for_service(..)` (:425) takes
  the full runtime-facing metadata (service, classification,
  lifecycle, state, restart count, last failure, sequence, time,
  detail). `with_startup_requirement(..)` attaches the validated
  startup-disposition policy; it is a builder rather than a new argument so an
  existing `for_service` caller keeps the `Required` default instead of
  silently acquiring optional startup semantics. Accessors:
  `service_name`, `classification`, `startup_requirement`, `lifecycle`,
  `health`, `restart_count`, `last_failure`, `state`, `transition_sequence`,
  `transition_time`, `is_live`, `is_ready`, `detail`.

### Shutdown / cancellation

- `enum ShutdownReason` (512) — `Requested / Signal / FatalFailure /
  Configuration / Test`.
- `struct CancellationToken` (527) — newtype over `Arc<AtomicBool>`,
  `Clone + Debug + Default`. Only `cancel` (Release store) and
  `is_cancelled` (Acquire load). One-way: there is no reset, no
  waker, and no async integration.

### Resource governor

- `enum ResourceClass` (546) — 17 classes, in declaration order:
  `ServiceTasks`, `ChildTasks`, `CommandQueueItems`,
  `EventQueueItems`, `BufferedBytes`, `SimulatedStreamLinks`,
  `SimulatedDatagramLinks`, `PendingTimers`, `TestPeers`, `Tasks`,
  `PendingHandshakes`, `ActiveLinks`, `NetDbQueries`,
  `TunnelBuilds`, `Destinations`, `Streams`, `ApiSessions`.
  `Tasks` is explicitly the legacy aggregate task count retained for
  existing callers (:565).
- `ResourceClass::ALL: [Self; 17]` (585) and
  `ResourceClass::COUNT: usize` (606).
- `struct ResourceLimit` (611) — public `class` / `maximum: u64`;
  `new` rejects zero with `ZeroLimit`.
- `struct ResourceRequest` (631) — public `class` / `amount: u64`;
  `new` rejects zero with `ZeroRequest`.
- `struct ResourceUsage` (651) — public `class`, `used`, `limit`,
  `high_water`, `denied`, `release_underflow`, plus const accessors
  `high_water_mark`, `denied_count`, `release_underflow_count`.
  `denied` and `release_underflow` saturate at `u64::MAX`.
- `struct ResourceBudget` (705) — `Arc<BudgetInner>` holding a
  `Mutex<BTreeMap<ResourceClass, ClassState>>`. `Clone + Debug`;
  limits are immutable after `new`. Methods: `new`,
  `try_acquire(request) -> ResourceLease`,
  `try_acquire_bundle(iter) -> ResourceBundle` (generic over
  `R: Borrow<ResourceRequest>`), `usage(class) -> ResourceUsage`,
  `snapshot() -> Vec<ResourceUsage>`.
- `struct ResourceBundle` (973) — `len`, `is_empty`, `iter` (grants in
  ascending `ResourceClass` order), `release`.
- `struct ResourceLease` (1001) — `Drop` releases its units;
  `class()`, `amount()`, `release()`.
- `enum ResourceError` (1034) — 10 variants: `ZeroLimit`,
  `ZeroRequest`, `DuplicateLimit`, `DuplicateRequest`,
  `EmptyBundle`, `TooManyClasses`, `MissingLimit`, `Exhausted
  { class, requested, available }`, `ArithmeticOverflow
  { class, used, requested }`, `Poisoned`.

## Key contracts

The crate defines **zero traits**. All contracts are concrete
structs/enums with methods.

- **Lifecycle FSM**: `LifecycleState::transition` (:103) accepts any
  self-transition plus this graph:
  `Registered → {WaitingForDependencies, Starting, Stopping}`;
  `WaitingForDependencies → {Starting, Stopping, Failed}`;
  `Starting → {Ready, Degraded, Stopping, Failed}`;
  `Ready → {Degraded, Stopping, Failed}`;
  `Degraded → {Ready, Stopping, Failed}`;
  `Stopping → Stopped`; `Failed → Stopping`. Anything else returns
  `InvalidLifecycleTransition`. `is_terminal()` (:132) covers
  `Stopped | Failed`.
- **Bound constants** are enforced at the constructor / admission
  layer, never at the type level: `ServiceName::new` and
  `HealthDetail::new` reject oversized values, and
  `ResourceBudget::new` / `try_acquire_bundle` reject more than
  `MAX_RESOURCE_CLASSES` classes.
- **Health reporting**: `HealthSnapshot` is an immutable value
  object; liveness and readiness delegate to `HealthState`.
  `HealthDetail` cannot leak through `Debug` because the derive is
  replaced by a redacting impl.
- **Cancellation**: `CancellationToken` is only
  `Arc<AtomicBool>` — no waker, no async, no `select!` integration.
  Plan 021's closure records it as "the polling-only token for
  synchronous contracts" (`plans/closure/workspace-foundation/021-closure.md:34`);
  `i2pr-runtime` layers the wakeable
  `tokio_util::CancellationToken` on top.
- **`ResourceBudget` accounting** is `Mutex`-guarded (not
  `RwLock`) over a `BTreeMap`, so `snapshot()` ordering is
  deterministic by `ResourceClass` ordering.
- `try_acquire` grants a single-class lease; a request that would
  exceed the limit returns `Exhausted` and bumps `denied`; a
  `u64` overflow returns `ArithmeticOverflow` and also bumps
  `denied`. `high_water` never decreases on release.
- `try_acquire_bundle` copies, sorts, and pre-validates requests
  (duplicate class → `DuplicateRequest`, empty → `EmptyBundle`,
  missing limit → `MissingLimit`, all before any mutation), then
  computes every `next` value into a pre-sized `Vec` and only then
  applies them — so an exhausted class leaves every other class
  untouched while still counting its own denial.
- **`ResourceLease` releases on `Drop`** (RAII), so receive, drop,
  error, cancel, and unwind paths all release identically.
- `ResourceBudget::release` (:946) recovers a poisoned mutex with
  `into_inner()`: a lease drop during unwinding is best-effort
  cleanup and must not leak a grant. Releasing more than is held
  clamps `used` to `0` and increments `release_underflow` instead
  of wrapping or panicking.
- `try_acquire`, `usage`, and `snapshot` surface a poisoned lock as
  `ResourceError::Poisoned`; only the release path recovers it.

## Errors

Four error types, all `Display + std::error::Error`:
`ServiceNameError` (:60), `InvalidLifecycleTransition` (:139),
`HealthDetailError` (:369), and `ResourceError` (:1034). The crate
uses no `anyhow`-style dynamic errors and never carries a
`Box<dyn Error>`.

`HealthDetail` implements `Debug` manually (:340) and prints
`HealthDetail { redacted: true }`, preventing diagnostic strings from
reaching logs even through `{:?}`. `ServiceFailure` and
`HealthSnapshot` derive `Debug` but only ever contain a redacting
`HealthDetail` and bounded/static values.

## Dependencies

**Zero dependencies, production or dev.** `Cargo.toml` declares no
`[dependencies]` and no `[dev-dependencies]` block. Only `std` is
used:

- `std::borrow::Borrow`
- `std::collections::BTreeMap`
- `std::fmt`
- `std::sync::atomic::{AtomicBool, Ordering}`
- `std::sync::{Arc, Mutex}`
- `std::time::Duration`

(The test module additionally uses `std::panic::{catch_unwind,
AssertUnwindSafe}`, `std::sync::atomic::AtomicUsize`,
`std::sync::Barrier`, and `std::thread`.)

This zero-dep invariant is **checker-enforced**:
`scripts/check-dependency-direction.sh:16` pins
`"i2pr-core": set()`. `i2pr-core` is the leaf of the production
dependency graph. Actual workspace dependents (per `cargo metadata`
and the same allowlist): `i2pr-transport`, `i2pr-tunnel`,
`i2pr-runtime`, `i2pr-client`, `i2pr-daemon`, and `i2pr-testkit`.
`i2pr-transport-ntcp2` does **not** depend on it — it reaches these
types only through `i2pr-transport`.

## Tests

There is **no `crates/i2pr-core/tests/` directory** and no test-only
dev-dependency. All coverage is the single inline
`#[cfg(test)] mod tests` at `lib.rs:1105-1425`: **14 synchronous
`#[test]` functions**, all on the current thread except the
explicitly threaded concurrency test. Line numbers below are the
`fn` definition lines.

| Test | Line | Coverage |
| --- | --- | --- |
| `lifecycle_rejects_recovery_from_stopped` | 1114 | FSM rejection + `Registered → Starting` |
| `health_snapshot_exposes_typed_readiness` | 1129 | Snapshot liveness/readiness via `HealthState` |
| `resource_lease_releases_on_drop_and_rejects_overcommit` | 1142 | RAII + overcommit rejection |
| `resource_classes_and_snapshots_are_bounded_and_deterministic` | 1154 | `COUNT`/`ALL`/`MAX_RESOURCE_CLASSES` + snapshot ordering |
| `resource_usage_records_exact_limit_denial_and_high_water` | 1182 | Exact-limit grant, `Exhausted` payload, high-water retention |
| `resource_validation_rejects_zero_and_handles_u64_overflow` | 1213 | `ZeroLimit`/`ZeroRequest` + `u64` overflow |
| `resource_release_is_consuming_drop_safe_and_unwind_safe` | 1249 | `catch_unwind` lease cleanup |
| `invalid_release_is_visible_without_wrapping_or_panicking` | 1278 | `release_underflow` counter via `release_for_test` |
| `resource_bundle_is_atomic_sorted_and_releases_together` | 1294 | Bundle atomicity, class sort order, joint release |
| `resource_bundle_rejects_duplicates_without_mutation` | 1339 | `DuplicateRequest` / `EmptyBundle`, no mutation |
| `concurrent_acquisition_never_exceeds_the_limit` | 1359 | 16 threads, limit 4, barrier-synchronised exact grant count |
| `bounded_types_reject_oversized_values` | 1405 | Both byte ceilings + 1 |
| `health_detail_debug_is_redacted` | 1411 | `Debug` redaction |
| `cancellation_is_shared_by_clones` | 1419 | Clones share the flag |

Honest gaps: no test covers `try_acquire_bundle` returning
`TooManyClasses`, `MissingLimit`, or `ArithmeticOverflow`; none
covers `ResourceError::Poisoned`; none covers `ServiceCompletion::
category`/`is_failure`, `ServiceName::as_str`/`Display`, or
`ServiceNameError::Empty`.

## Distinctive design choices

- **Zero dependencies** — uncommon and deliberate; the crate pulls
  only `std` primitives and CI fails on any addition.
- **No traits at all** — every contract is concrete, so runtime code
  consumes these types directly with no generic abstraction layer.
- **`HealthDetail::Debug` is hand-written and redacting**, so a
  derive could never be re-added by accident without a semantic
  diff.
- **`CancellationToken` is intentionally minimal and polling-only** —
  just `Arc<AtomicBool>` with `Release`/`Acquire` ordering; the
  runtime layer owns the wakeable variant.
- **Bundle atomicity without a second lock or a transaction log** —
  pre-validation plus a pre-sized `Vec` of `next` values, so a
  partial admission is structurally impossible.
- **Sort-then-scan gives deadlock-free, deterministic order** —
  `BTreeMap`/`Ord` class ordering is what makes `snapshot()` stable.
- **Accounting faults are counted, not fatal** — over-release clamps
  to zero and increments `release_underflow` rather than wrapping or
  panicking.
- **Release is best-effort by design** — a poisoned lock is recovered
  with `into_inner()` so a lease dropped during unwinding cannot
  leak its grant, while the read paths report `Poisoned`.
- **`ServiceName` exposes only `AsRef<str>` / `as_str` / `Display`** —
  deliberately no `AsRef<[u8]>` and no `From<String>` escape hatch.
- **Two snapshot constructors** — `new` derives `LifecycleState` from
  `HealthState` for callers that only know health; `for_service` is
  the explicit runtime-facing constructor.

## Cross-references

- [Overview](overview.md)
- [i2pr-runtime.md](i2pr-runtime.md) — adds the wakeable
  cancellation layer and supervision. It uses 25 of this crate's 29
  public items: `LifecycleState`, `InvalidLifecycleTransition`,
  `ServiceName`, `ServiceNameError`, `ServiceClassification`,
  `StartupRequirement`,
  `ServiceCompletion`, `ServiceFailure`, `ServiceFailureCategory`,
  `FailureCategory`, `HealthState`, `HealthSnapshot`, `HealthDetail`,
  `MAX_HEALTH_DETAIL_BYTES`, `DegradationCode`, `ShutdownReason`,
  `CancellationReason`, and the `ResourceBudget` / `ResourceClass` /
  `ResourceLimit` / `ResourceRequest` / `ResourceUsage` /
  `ResourceLease` / `ResourceError` governor surface.
- [i2pr-transport.md](i2pr-transport.md) — re-exports eight governor
  types at its crate root (`crates/i2pr-transport/src/lib.rs:11-14`).
- [i2pr-testkit.md](i2pr-testkit.md) — uses `ResourceBudget` with
  `PendingTimers`, `BufferedBytes`, and simulated stream/datagram
  link leases (`crates/i2pr-testkit/src/network.rs:657-668`).
- [ADR 0003: Bounded supervised services and explicit cancellation](../adr/0003-bounded-supervised-services.md).
- [ADR 0008: Concrete runtime ownership and wakeable supervision](../adr/0008-runtime-supervision-and-cancellation.md).
- [ADR 0009: Privacy-aware runtime observability and deterministic validation](../adr/0009-runtime-observability-and-validation.md).
- Plan of record: Plan 021 (supervision, wakeable cancellation, and
  shutdown) created the lifecycle/health/cancellation surface;
  Plan 022 (bounded channels, backpressure, and resource governance)
  added the resource classes and governor. Both closed as "Complete
  for the bounded, non-networked scope"
  (`plans/closure/workspace-foundation/021-closure.md`,
  `022-closure.md`); Plan 023 added the deterministic testkit
  consumer. See `plans/README.md` for the registry.
