# Plan 021 closure: supervision, wakeable cancellation, and shutdown

- Status: Complete for the bounded non-networked scope
- Date: 2026-07-15
- Plan: [`021-m2-supervision-cancellation.md`](021-m2-supervision-cancellation.md)

## Scope and deviations

Plan 021 is implemented as a concrete `i2pr-runtime` crate over
`i2pr-core`. The implementation remains protocol-agnostic and non-networked:
it opens no sockets, performs no DNS or NetDB work, constructs no tunnels,
exposes no client/API listeners, and advertises no protocol capability.

Independent services start sequentially in deterministic dependency order.
This is an intentional bounded deviation from optional concurrent startup so
tests do not depend on unspecified scheduler poll order. Restart handling is
owned by each service manager; only `Restartable` services may restart and
restart exhaustion explicitly degrades or shuts down. The runtime uses
`tokio-util`'s reviewed hierarchical cancellation primitive with a local
first-reason-wins reason store rather than adding a custom async token.

Per-service graceful periods contribute to the bounded supervisor shutdown
deadline. A later plan may refine independent per-service deadlines without
changing ownership or requiring detached task cleanup. `i2pr-daemon` depends on
the runtime as the composition root, but the existing live CLI command remains
disabled and no runtime startup path is exposed to operators yet.

## Changed files

- `Cargo.toml`, `Cargo.lock`: add the bounded Tokio, `tokio-util`, and
  `futures-util` workspace dependencies and the runtime workspace member.
- `crates/i2pr-core/src/lib.rs`: add service classifications, lifecycle phases,
  typed completion/failure categories, bounded cancellation reasons, and full
  runtime health snapshot metadata while retaining the polling-only token for
  synchronous contracts.
- `crates/i2pr-runtime/Cargo.toml`, `src/lib.rs`, `src/cancel.rs`,
  `src/context.rs`, `src/graph.rs`, `src/supervisor.rs`: concrete runtime
  boundary, graph validation, wakeable cancellation, readiness/health,
  child-task scope, restart manager, and graceful/forced shutdown report.
- `crates/i2pr-daemon/Cargo.toml`: allow the daemon composition root to depend
  on `i2pr-runtime` without making the current CLI live.
- `crates/i2pr-daemon/src/cli.rs`, `src/error.rs`: clarify that the concrete
  runtime exists but live daemon composition is intentionally not enabled.
- `crates/i2pr-daemon/tests/cli.rs`: keep the CLI regression aligned with the
  clarified non-networked live-run message.
- `scripts/check-dependency-direction.sh`: enforce
  `i2pr-core <- i2pr-runtime <- i2pr-daemon` and keep protocol/crypto/storage
  runtime-free.
- `README.md`, `GUARDRAILS.md`, `AGENTS.md`, `CONTRIBUTING.md`: update current
  status, ownership rules, deterministic runtime testing, and dependency
  boundaries.
- `docs/architecture.md`, `docs/security-model.md`,
  `docs/protocol-support.md`: document runtime ownership, task/cancellation
  threats, shutdown evidence, and the fact that runtime infrastructure is not
  protocol support evidence.
- `docs/adr/0008-runtime-supervision-and-cancellation.md`: record runtime,
  cancellation, feature, MSRV, dependency, security, and rejected-alternative
  decisions.
- This file: closure evidence and handoff.

## Public contracts and limits

- `ServiceName` is bounded to 64 UTF-8 bytes.
- A graph accepts at most 128 services and validates all dependencies before
  spawning; duplicate, missing, self, cyclic, and nondeterministic graphs are
  rejected.
- Registration deadlines and restart delays are nonzero and capped at one
  hour. Restart policies allow at most 32 replacement attempts and reject
  zero-delay hot loops. Child scopes allow at most 64 children.
- Four classifications are explicit: `Essential`, `Restartable`, `Degradable`,
  and `Optional`. Restart policy on any other classification is rejected.
- Lifecycle transitions represent `Registered`,
  `WaitingForDependencies`, `Starting`, `Ready`, `Degraded`, `Stopping`,
  `Stopped`, and `Failed`; invalid transitions return typed errors.
- Readiness is one-shot per service instance. Health uses bounded latest-state
  snapshots containing service identity, classification, lifecycle, health,
  restart count, static failure category, sequence, monotonic transition time,
  and optional bounded detail.
- Completion categories include requested shutdown, unexpected clean exit,
  typed service failure, panic, join failure, startup/readiness timeout,
  graceful timeout, forced abort, and restart-budget exhaustion. Panic payloads
  and raw errors are not retained.
- `i2pr-runtime::CancellationToken` is wakeable, hierarchical, idempotent, and
  first-reason-wins. Child cancellation cannot cancel a parent. The core atomic
  token remains explicitly synchronous and polling-only.
- All manager tasks are owned by a `JoinSet`. Child scopes own child `JoinSet`s;
  service completion joins children, while scope drop aborts remaining child
  work as a final cleanup guard.
- Shutdown returns a report distinguishing fully graceful and partially forced
  cleanup, with final typed completions, joined-task count, and zero remaining
  tasks after drain.

## Test evidence

The runtime unit tests cover:

- deterministic topological order, duplicate/missing/self/cyclic dependency
  validation, essential-graph policy, service-count limits, timeout bounds,
  and restart-policy classification;
- cancellation before wait, cancellation during wait, multiple waiters,
  parent propagation, child isolation, idempotence, and first-reason-wins;
- readiness and graceful startup/shutdown, panic classification and payload
  redaction, bounded restart backoff/recovery, and forced abort of a
  non-cooperative service;
- core lifecycle, health bounds, cancellation sharing, and resource lease
  release regressions.

All asynchronous timing tests use paused Tokio time and explicit
`tokio::time::advance`; no wall-clock sleeps or network activity are used.
The focused supervisor lane passed five repeated runs (14 tests per run) with
`--test-threads=1`. The full workspace test run passed 94 tests across 17
suites. The tests use Tokio's paused clock and explicit time advancement; no
random scheduling seed is applicable to this test lane.

## Dependency and security decisions

Tokio is enabled only with `macros`, `rt`, `sync`, `time`, and `test-util`;
`tokio-util` uses only `rt`; `futures-util` uses only `std` for panic
classification. No network features enter the dependency graph. `cargo deny`
and MSRV checks remain required release gates.

The supervisor treats task leaks, lost cancellation wakeups, hot restart loops,
panic disclosure, raw health diagnostics, and forced-cleanup ambiguity as
security-relevant failure modes. Names, details, counts, timeouts, attempts,
and reasons are bounded. No protocol support, anonymity, network safety, or
production-readiness claim follows from this runtime work.

## Quality results

The local validation results for this closure are recorded after the final
documentation and code review pass:

```text
cargo fmt --all --check                         PASS
cargo check --workspace --all-targets           PASS
cargo test --workspace                          PASS
cargo clippy --workspace --all-targets --all-features -- -D warnings PASS
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps PASS
bash scripts/check-dependency-direction.sh      PASS
cargo deny check advisories bans sources        PASS (pre-existing rand_core duplicate warning)
cargo +1.85.0 check --workspace --all-targets   PASS
git diff --check                                PASS
```

Remote CI evidence: pushed commit `3c8137b` passed GitHub Actions CI run
`29401961187`, including Quality on Ubuntu and macOS, Ubuntu MSRV, and
dependency-policy advisories/bans/sources. GitHub reported only the existing
Node.js action deprecation annotations; no job failed.

## Known limitations and Plan 022 handoff

- The daemon does not yet wire a live `run` command to the runtime.
- No bounded asynchronous command/event channel or router-wide resource
  governor is included; those are Plan 022 contracts.
- Health is latest-state only and has no event history or tracing subscriber.
- Per-service grace periods are bounded through the supervisor deadline but
  are not yet independent abort timers.
- Runtime replacement is not claimed to be portable; the concrete Tokio API is
  intentionally localized to `i2pr-runtime`.
- No network/protocol interoperability, public-network, anonymity, or
  production-readiness evidence exists.

Plan 022 should preserve the dependency direction and task ownership model,
define bounded channel overflow semantics, integrate resource leases with
service scopes, and add release-on-timeout/panic/cancellation evidence without
introducing detached tasks.
