# Milestone 2 targeted corrective closure plan

## Purpose

Correct the remaining lifecycle, ownership, reviewability, CI, and accounting-invariant issues identified after the Milestone 2 implementation review, without broadening scope into NTCP2, live networking, daemon activation, NetDB, tunnels, destinations, client APIs, or protocol capability advertisement.

This is a targeted corrective pass over the completed Milestone 1 and Milestone 2 foundations. It is not a new feature milestone. Its purpose is to make the existing closure claims technically exact before Milestone 3 transport planning or implementation begins.

## Governing sources

The implementation must remain consistent with:

- `GUARDRAILS.md`
- `AGENTS.md`
- `CONTRIBUTING.md`
- `docs/architecture.md`
- `docs/security-model.md`
- `docs/adr/0008-runtime-supervision-and-cancellation.md`
- `docs/adr/0009-runtime-observability-and-validation.md`
- `plans/020-milestone-2-closure.md`
- `plans/021-closure.md`
- `plans/022-closure.md`
- `plans/023-closure.md`
- `plans/024-m2-observability-validation-closure.md`

If a correction requires weakening an existing ownership, boundedness, privacy, or no-network invariant, stop and record the conflict instead of silently changing the contract.

## Current findings to correct

### Finding A: forced child-task cleanup may report zero before task termination

The normal service-attempt path calls `ChildScope::shutdown().await`, cancels the scope, joins every child, and decrements counters after each join.

The forced path currently relies on `ChildScopeInner::drop`, which calls `JoinSet::abort_all()` and immediately decrements the child counters according to the current set length. The dropped `JoinSet` is not asynchronously drained by an explicit owner before the aggregate snapshot may report zero.

This means the observability counter can describe ownership bookkeeping rather than confirmed task termination. Milestone 2 requires stronger evidence: every owned child task must be joined after ordinary completion or after abort completion before shutdown reports and final snapshots claim zero remaining child tasks.

### Finding B: an essential service may return `RequestedShutdown` without cancellation

A service may currently return `ServiceResult::RequestedShutdown` even when neither its service token nor the root token was cancelled. That result is classified as a non-failure completion, allowing an essential service to disappear while the remaining graph continues.

`RequestedShutdown` must describe an observed cancellation-driven shutdown, not an arbitrary service-selected clean exit. An uncancelled essential or restartable service returning that value must be reclassified as an unexpected exit or a dedicated invalid-completion failure.

### Finding C: protocol module decomposition is primarily a façade

Milestone 1 added domain-named leaf modules, but most implementation code remains in `common_impl.rs` and `i2np_impl.rs`. The public façade is useful, but the implementation still has large edit and review surfaces.

Before transport, NetDB, and tunnel milestones create concurrent work around these types, the implementation should be physically moved into domain-owned modules while preserving the crate-root public API and wire behavior.

### Finding D: corrective mechanical checks are not all CI gates

`scripts/check-runtime-boundaries.sh` and `scripts/check-fixture-manifest.sh` are local validation tools but are not both enforced by the normal quality workflow.

A future change can therefore violate a closure invariant while ordinary CI remains green.

### Finding E: resource release silently masks underflow

`ResourceBudget::release` uses `saturating_sub`, which protects unwinding from panic but can conceal a double release or accounting defect. Because resource leases are intentionally non-cloneable and release exactly once, an underflow indicates an internal invariant violation and must become visible without making cleanup fragile.

## Scope

This corrective pass may modify:

- `crates/i2pr-runtime/src/context.rs`
- `crates/i2pr-runtime/src/supervisor.rs`
- `crates/i2pr-runtime/src/observability.rs`
- focused runtime tests
- `crates/i2pr-core/src/lib.rs`
- focused resource-governor tests
- `crates/i2pr-proto/src/common/`
- `crates/i2pr-proto/src/i2np/`
- `crates/i2pr-proto/src/common_impl.rs`
- `crates/i2pr-proto/src/i2np_impl.rs`
- protocol tests and fuzz imports where paths change
- `.github/workflows/ci.yml`
- `scripts/check-runtime-boundaries.sh`
- `scripts/check-fixture-manifest.sh`
- architecture, security, contributor, agent, ADR, and closure documentation
- a new corrective closure record

No public operator behavior, socket use, DNS, reseeding, transport framing, handshake logic, NetDB operation, tunnel construction, destination service, SAM, I2CP, proxy, or capability advertisement may be added.

## Dependency and architecture constraints

- `i2pr-runtime` remains the only production crate that owns Tokio tasks, timers, channels, and wakeable cancellation.
- `i2pr-core` remains runtime-neutral and Tokio-free.
- `i2pr-testkit` remains a non-production test/simulation dependency.
- Protocol decomposition must not introduce runtime, filesystem, CLI, tracing-subscriber, or crypto-execution dependencies into `i2pr-proto`.
- Do not introduce a generalized executor abstraction, a second supervisor, a global task registry, a service locator, a universal codec trait, or a generic secret-management framework.
- Do not use detached tasks as a cleanup mechanism.
- Do not convert task counters into the authoritative ownership mechanism. Join ownership remains authoritative; counters are observations derived from it.

## Execution order

Execute the phases below in order. Phase C may be performed in a separate commit after runtime correctness is green, but Milestone 3 must not begin until all phases close.

1. Forced child-task ownership and drain correction.
2. Essential-service completion classification correction.
3. Physical protocol implementation decomposition.
4. CI and mechanical guardrail integration.
5. Resource underflow detection and final closure evidence.

## Phase A: forced child-task ownership and drain correction

### Objective

Ensure that no shutdown report, runtime snapshot, or task counter reports zero owned child tasks until every child future has either completed normally or completed cancellation after explicit abort and join.

### Required design properties

The corrected design must satisfy all of the following:

- Every spawned child task has one auditable async owner.
- Normal service completion cancels and joins all children.
- Service panic, readiness timeout, startup timeout, and service failure also cancel and join all children.
- Forced supervisor cleanup may abort a manager, but the manager's child tasks must remain owned by an object that is itself joined or explicitly drained.
- Dropping the last `ChildScope` handle must not fabricate task completion by decrementing counters before join completion.
- Aggregate task counters are decremented only after a corresponding join result, or after an explicitly awaited abort-drain operation confirms termination.
- Shutdown remains bounded by the supervisor's global deadline.
- A non-cooperative child cannot keep shutdown alive indefinitely.
- The final report distinguishes child cleanup failure from successful forced cleanup if the runtime cannot confirm termination.

### Preferred implementation direction

Prefer one of these ownership-preserving approaches:

#### Option 1: manager-owned child drain guard

- Keep the child `JoinSet` inside the service manager's async state.
- Ensure the manager catches service completion and always executes a bounded child shutdown phase before it can return.
- On supervisor deadline, cancel the manager first.
- If the manager does not complete, abort the manager and move a separately owned child-drain handle to the supervisor for `abort_all` plus `join_next` draining.

#### Option 2: supervisor-visible child registry with transfer semantics

- Each service manager registers one bounded child collection owner with the supervisor.
- The manager owns it during ordinary execution.
- Forced manager abort atomically transfers ownership to the supervisor cleanup path.
- The supervisor aborts and drains that exact collection before final reporting.

Do not implement a global unrestricted task registry. The ownership transfer must remain per service, bounded by `MAX_CHILD_TASKS`, and private to `i2pr-runtime`.

### Drop semantics

`Drop` may remain a last-resort abort request, but it must not:

- decrement confirmed-finished counters;
- claim cleanup completion;
- discard handles without an owner that will drain them;
- block or perform asynchronous work.

If a dropped scope cannot transfer handles safely, record an internal cleanup fault and ensure the final supervisor outcome is `FailedCleanup` rather than a false success.

### Reporting changes

Consider adding bounded fields to the shutdown or supervisor evidence model:

- child tasks joined normally;
- child tasks aborted and joined;
- child cleanup failures;
- child tasks remaining after final drain.

Do not expose task payloads, panic text, dynamic type names, or arbitrary errors.

### Required tests

Add deterministic paused-time tests for:

1. A service with multiple cooperative children shuts down and all joins complete.
2. A service returns normally while a child is still running; the manager cancels and joins the child before returning.
3. A service panics while children are running; children are cancelled and joined.
4. A readiness timeout occurs while children are running; children are drained.
5. An essential service fails while another service has a non-cooperative child; the global deadline forces abort, then the child abort is joined before the report returns.
6. A manager itself is forcibly aborted; child ownership transfers or drains correctly.
7. Snapshots observed during cleanup never show fewer owned children than have actually completed termination.
8. Final `ShutdownReport`, `SupervisorSnapshot`, and task counters all report zero only after confirmed joins.
9. A deliberate cleanup-invariant fault produces `FailedCleanup` or another typed failure rather than a false graceful/partially-forced result.
10. Repeat the forced-cleanup test at least 100 times under `--test-threads=1` or an equivalent deterministic loop to expose ownership races.

No test may use wall-clock sleeps.

## Phase B: essential-service completion classification

### Objective

Make service completion semantics reflect observed runtime state rather than trusting a service-supplied enum variant.

### Required rules

- `ServiceResult::RequestedShutdown` is valid only when the service token, manager token, or root token is already cancelled for a recognized shutdown reason.
- If no relevant token is cancelled:
  - an `Essential` service returning `RequestedShutdown` is an unexpected clean exit or invalid completion and must fail the graph;
  - a `Restartable` service returning it is an unexpected exit eligible for its explicit restart policy;
  - a `Degradable` service returning it is recorded as a typed failure/degradation;
  - an `Optional` service returning it is recorded as an unexpected exit, not a requested shutdown.
- A genuinely cancellation-driven service returning `RequestedShutdown` remains a clean requested completion.
- A service returning `Completed` while cancelled may remain classified as requested shutdown only if that behavior is explicitly documented and tested; otherwise preserve the distinction between cooperative requested shutdown and generic completion after cancellation.
- Completion classification must not depend on human-readable detail text.

### Suggested implementation

Centralize classification in one private function, for example:

```text
classify_service_result(result, cancellation_state, classification) -> ServiceCompletion
```

The function should receive only the typed data it needs and should be used for both pre-readiness and post-readiness service completion paths. Avoid duplicating classification branches in `run_attempt`.

### Required tests

Add tests for every service classification:

- uncancelled `RequestedShutdown`;
- root-cancelled `RequestedShutdown`;
- service-token-cancelled `RequestedShutdown`;
- uncancelled `Completed`;
- cancelled `Completed`;
- service failure;
- panic.

At minimum, verify that an essential service cannot disappear while the supervisor remains `Ready`, and that a restartable service with an uncancelled pseudo-requested shutdown consumes restart budget instead of silently exiting.

## Phase C: physical protocol implementation decomposition

### Objective

Move the implementation bodies out of the two large compatibility files into domain-owned modules without changing the public API, encoded bytes, error taxonomy, resource bounds, or support claims.

### Target structure

A reasonable target is:

```text
crates/i2pr-proto/src/
  common/
    mod.rs
    date.rs
    hash.rs
    mapping.rs
    certificate.rs
    keys.rs
    identity.rs
    router_address.rs
    router_info.rs
    lease.rs
    lease_set.rs
    shared.rs          # only narrowly shared private helpers
  i2np/
    mod.rs
    header.rs
    message.rs
    delivery.rs
    netdb.rs
    tunnel.rs
    deferred.rs
    codec.rs           # only I2NP-specific private dispatch/helpers
```

The exact filenames may differ, but implementation ownership must become real rather than only re-exported.

### Migration rules

- Preserve all existing crate-root exports unless an existing export is demonstrably accidental and a compatibility note is recorded.
- Keep top-level decoder behavior exact.
- Keep all existing numeric bounds unchanged unless a separate specification correction is required.
- Do not broaden helper visibility merely to make the move easier.
- Prefer `pub(super)` or private shared helpers over crate-wide `pub(crate)`.
- Avoid cyclic module dependencies by moving the smallest shared primitives to a narrowly named private module.
- Keep signed-region ownership and canonical-encoding boundaries unchanged.
- Keep `ReplySecret` non-cloneable and zeroizing.
- Do not mix transport, NetDB policy, freshness policy, crypto execution, or runtime behavior into the protocol modules.
- Remove `common_impl.rs` and `i2np_impl.rs` after all implementations have moved. Do not leave them as permanent compatibility warehouses.

### Validation requirements

Before and after the move, capture and compare:

- all public API tests;
- all fixed positive fixtures;
- all malformed fixture error categories;
- canonical re-encoding bytes;
- fuzz target compilation;
- rustdoc output;
- `cargo public-api` output if the tool is already available or can be used without adding a project dependency.

Do not add a production dependency solely for API comparison.

### Required tests

No new semantic support is required, but the existing fixture-backed integration suite must continue to consume every manifest row. Add focused module-boundary tests only where moving helpers exposes previously implicit coupling.

## Phase D: CI and mechanical guardrail integration

### Objective

Make the repository's corrective invariants continuously enforced on every push and pull request.

### Required CI changes

Add the following to the normal Ubuntu quality job, or to a dedicated lightweight policy job:

```text
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

Also ensure CI runs:

```text
cargo check --workspace --all-targets
```

rather than only the default-target workspace check.

The macOS quality job may retain the existing general matrix, but at least one Linux job must run all mechanical scripts and all targets.

### Improve `check-runtime-boundaries.sh`

The script should mechanically reject, within `i2pr-runtime` and `i2pr-testkit`:

- unbounded Tokio channels;
- detached `tokio::spawn` patterns outside explicitly approved ownership sites;
- `std::mem::forget` on join handles or scopes;
- wall-clock sleeps in tests;
- production dependencies on `i2pr-testkit`;
- direct Tokio dependencies outside approved crates;
- use of `JoinHandle` without documented ownership where a reliable static pattern can be checked.

Do not make the script so broad that it creates unstable false positives. Each pattern should be narrow, documented, and backed by a small self-test or fixture if practical.

### Fixture manifest gate

The fixture check must continue validating:

- every `.hex` fixture appears exactly once in the manifest;
- every manifest path exists;
- hashes match;
- classification and provenance fields are present;
- no unmanifested fixture is silently ignored.

### CI evidence

The corrective closure is not complete until a fresh GitHub Actions run on the final commit passes:

- Ubuntu quality;
- macOS quality;
- Rust 1.85 MSRV;
- dependency policy;
- runtime-boundary and fixture-manifest checks.

Record the exact run ID in the closure file.

## Phase E: resource underflow detection

### Objective

Keep cleanup non-panicking while making impossible release underflow visible as an internal invariant failure.

### Required behavior

- Ordinary valid lease release remains infallible to callers.
- Cleanup during unwinding must not panic.
- A release amount greater than current usage must not silently normalize to zero without evidence.
- The budget should record a bounded, saturating internal-invariant counter or fault flag.
- Debug builds should use `debug_assert!` where safe and useful.
- Runtime snapshots or explicit diagnostics may expose only a typed invariant count/flag, not dynamic error text.
- An invariant fault must not permit usage to wrap.

### Suggested design

Extend per-class accounting with one field such as:

```text
release_underflow: u64
```

On release:

- if `amount <= used`, subtract normally;
- otherwise increment `release_underflow` with saturation, set `used = 0` as the safest bounded recovery, and trigger a `debug_assert!` in debug builds;
- never panic in release builds or during unwinding.

Alternatively, use one budget-wide typed invariant counter if per-class detail is unnecessary. Avoid storing call-site text or backtraces.

### Required tests

- Valid drop release leaves the invariant counter at zero.
- Consuming release leaves it at zero.
- Bundle drop leaves every class at zero and no invariant faults.
- Panic unwind release leaves no leak and no underflow.
- A private test-only fault injection of an invalid release increments the typed invariant signal and does not wrap or panic in release-mode-equivalent behavior.
- Concurrent acquire/release tests leave all usage at zero and no invariant faults.

Do not expose a public arbitrary-release API merely to test this. Use a private test hook or module-local test access.

## Integrated corrective validation matrix

After all phases, add or update an integrated deterministic test covering:

1. Essential service starts and owns cooperative and non-cooperative children.
2. A bounded channel and resource lease are active.
3. The essential service exits through an uncancelled `RequestedShutdown` result.
4. The result is reclassified as failure.
5. Root cancellation begins.
6. Cooperative children join normally.
7. Non-cooperative children are aborted and their abort joins complete.
8. Queue entries and resource leases release.
9. Final supervisor and runtime snapshots show zero service tasks, zero child tasks, zero queue depth, and zero resource usage.
10. No release-underflow invariant is recorded.
11. The shutdown outcome correctly distinguishes graceful and forced components.
12. Repeating the scenario with the same deterministic schedule yields the same typed report and counters.

## Documentation updates

Update at minimum:

- `AGENTS.md`: authoritative child-task ownership and uncancelled completion rules.
- `CONTRIBUTING.md`: focused runtime corrective test commands and CI gates.
- `README.md`: current closure status without overstating router functionality.
- `GUARDRAILS.md`: no false zero-task reporting and no silent accounting-underflow masking.
- `docs/architecture.md`: final child ownership/transfer/drain model and physical protocol modules.
- `docs/security-model.md`: forced-abort race, false cleanup evidence, invalid requested-shutdown classification, and accounting-corruption handling.
- ADR 0008: amend the accepted supervision design with the final forced-child drain mechanism.
- ADR 0009: clarify that snapshots report confirmed termination, not only counter decrements.
- `plans/020-milestone-2-closure.md`: mark the prior closure as corrected/superseded where necessary and link the corrective record.

Do not rewrite history or remove the earlier closure evidence. Record the correction transparently.

## Corrective closure record

Create:

```text
plans/025-closure.md
```

It must include:

- implementation commits;
- exact changed files;
- final child-task ownership model;
- completion-classification truth table;
- final protocol module layout;
- resource underflow behavior;
- test inventory and deterministic repetition counts;
- exact local commands and results;
- final GitHub Actions run ID and job results;
- deviations;
- unresolved limitations;
- explicit Milestone 3 readiness statement.

## Required local commands

Run at minimum:

```text
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-runtime --all-targets
cargo test -p i2pr-testkit --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
git diff --check
```

Also run a deterministic repeated lane for the forced-child cleanup and uncancelled essential completion tests. Record the exact command and number of repetitions in `plans/025-closure.md`.

If protocol files move, also run:

```text
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
```

If the offline cache is unavailable, record the exact limitation and run the corresponding online lane in CI or an authorized environment. Do not claim fuzz compilation evidence without executing it.

## Acceptance criteria

This corrective plan closes only when all of the following are true:

- Forced child tasks are explicitly aborted and joined before final reports claim zero.
- Child-task counters reflect confirmed task termination rather than handle disposal.
- Cleanup failures produce typed failure evidence instead of false success.
- An essential service cannot disappear through an uncancelled `RequestedShutdown` result.
- Restartable, degradable, and optional classifications handle the same invalid completion consistently and explicitly.
- `common_impl.rs` and `i2np_impl.rs` are removed, with implementation physically owned by domain modules.
- Public protocol exports, fixture bytes, error categories, and support claims remain unchanged.
- Runtime-boundary and fixture-manifest scripts run in CI.
- CI checks all workspace targets on at least one platform.
- Resource release underflow becomes observable through a bounded typed invariant signal.
- Valid resource paths produce zero underflow signals.
- All local validation commands pass.
- A fresh remote CI run passes every required job.
- `plans/025-closure.md` exists and records exact evidence.
- `plans/020-milestone-2-closure.md` links the corrective closure and no longer presents superseded cleanup wording as final.
- No live network or Milestone 3 protocol behavior is introduced.

## Stop conditions

Stop and report rather than improvising if:

- Tokio `JoinSet` ownership cannot be transferred or drained without detached cleanup work.
- Correct forced child cleanup requires an unbounded global task registry.
- The supervisor deadline cannot bound both manager and child draining without changing the public shutdown contract materially.
- A proposed fix reports zero through counters without proving join completion.
- Service completion classification cannot distinguish cancellation state reliably.
- Physical protocol decomposition changes encoded bytes, signed regions, fixture results, or public support status.
- Moving protocol code requires broadening helper visibility across unrelated domains.
- CI scripts produce unstable false positives that cannot be narrowed.
- Accounting underflow detection would panic during unwinding or introduce a caller-controlled denial of service.
- A correction requires live sockets, public-network tests, or transport implementation.

## Milestone 3 gate

Do not begin NTCP2 implementation until this corrective closure is complete.

Milestone 3 planning may proceed only after the final closure demonstrates:

- authoritative parent/child task ownership;
- bounded cancellation and forced drain;
- exact essential-service failure semantics;
- stable protocol module ownership;
- continuously enforced runtime and fixture guardrails;
- resource accounting with visible invariant failures;
- green normal, MSRV, dependency-policy, and cross-platform CI.

The Milestone 3 handoff must cite `plans/025-closure.md` as a prerequisite and must not duplicate or bypass these lifecycle contracts.