# Milestone 2 Plan A: service supervision, wakeable cancellation, and shutdown

## Purpose

Implement the concrete asynchronous service lifecycle used by later transport, NetDB, tunnel, client, and API subsystems. This plan introduces supervised task ownership, dependency-aware startup, wakeable cancellation, health/readiness reporting, bounded restart behavior, and graceful/forced shutdown.

The implementation must remain protocol-agnostic and non-networked.

## Required inputs

- `plans/020-milestone-2-overview.md`
- aggregate Milestone 1 closure
- `docs/architecture.md`
- `docs/security-model.md`
- current lifecycle, health, cancellation, and resource types in `i2pr-core`
- current daemon composition and CLI behavior

## Scope

This plan may:

- add `crates/i2pr-runtime`;
- add narrowly reviewed Tokio and cancellation dependencies;
- refine runtime-neutral lifecycle and health vocabulary in `i2pr-core`;
- add a concrete supervisor and service graph;
- add integration tests using synthetic services;
- update architecture, security, contributor, and agent documentation;
- add ADRs for runtime selection and task-supervision policy.

This plan must not add live sockets, protocol state machines, NetDB behavior, tunnels, application listeners, or runtime-loadable plugins.

## Phase A: runtime and dependency ADR

Create an ADR covering:

- why a concrete `i2pr-runtime` crate is preferable to placing reusable supervision inside `i2pr-daemon`;
- why `i2pr-core` remains runtime-neutral;
- selected Tokio version and exact enabled features;
- selected cancellation implementation;
- Rust 1.85 compatibility;
- dependency maintenance and security posture;
- rejected alternatives, including a broad runtime trait and detached subsystem-owned spawning;
- how later runtime replacement would be localized without pretending runtime portability exists today.

Expected minimal Tokio capabilities are runtime, macros for tests, synchronization, and time. Do not enable network features in this milestone. If `tokio-util` is selected solely for cancellation, enable only the required feature set.

Update `scripts/check-dependency-direction.sh` before implementation code lands so the new allowed dependency edges are mechanically enforced.

## Phase B: runtime-neutral service contracts

Refine or add only the semantic types required by the supervisor.

### Service identity

Provide a bounded stable service identifier. It must:

- reject empty and excessive names;
- avoid attacker-controlled arbitrary cardinality;
- be usable as a health and trace field;
- not embed peer identities, destinations, IP addresses, or dynamic connection IDs.

Static service names are preferred for the initial graph.

### Service classification

Define explicit classifications:

- `Essential`: failure initiates coordinated router shutdown.
- `Restartable`: failure may restart under a bounded policy.
- `Degradable`: failure marks the router degraded but other services continue.
- `Optional`: failure is recorded but does not degrade required functionality.

Classification must be part of registration and immutable while a service instance is running.

### Lifecycle states

Reconcile the existing lifecycle model with at least:

- registered;
- waiting for dependencies;
- starting;
- ready;
- degraded;
- stopping;
- stopped;
- failed.

Not every state needs a public enum variant if health and lifecycle are deliberately separated, but the combined model must represent these conditions without contradictory snapshots.

Define legal transitions and test them as a table. Invalid transitions must return typed errors; they must not panic or silently rewrite state.

### Completion and failure

Define a bounded service completion result that distinguishes:

- clean requested shutdown;
- unexpected clean exit;
- typed service failure;
- panic or task join failure;
- startup timeout;
- readiness timeout;
- graceful-shutdown timeout;
- forced abort;
- restart budget exhaustion.

Errors must contain static categories and safe bounded details, not arbitrary payloads.

## Phase C: service registration and graph validation

Implement a registration model containing:

- service identifier;
- classification;
- explicit dependency identifiers;
- startup/readiness deadline;
- shutdown grace period;
- restart policy where allowed;
- task factory or owned future constructor;
- optional static description for diagnostics.

Graph validation must occur before any task starts.

Reject:

- duplicate service identifiers;
- missing dependencies;
- self-dependencies;
- dependency cycles;
- restart policy on a classification that cannot restart;
- zero or excessive timeouts;
- an empty essential graph where the caller requires at least one essential service;
- excessive service counts under an explicit router-wide maximum.

Return a deterministic topological startup order. Do not rely on hash-map iteration order.

## Phase D: wakeable cancellation

Replace polling-only cancellation at runtime-facing boundaries with a wakeable implementation.

Required semantics:

- cancellation is idempotent;
- cancellation before waiting completes the wait immediately;
- cancellation after waiting wakes all current waiters;
- child tokens inherit parent cancellation;
- child cancellation does not cancel a parent;
- a cancellation reason is recorded once and cannot be replaced by later callers;
- reasons are bounded and privacy-safe;
- dropping a handle does not implicitly cancel unrelated work;
- cancellation can participate in `select!` with command reception and deadlines;
- no lost-wakeup race is allowed.

The existing runtime-neutral atomic token may remain for synchronous polling contracts if its limitation stays explicit. It must not be passed off as the asynchronous service cancellation mechanism.

Suggested cancellation reasons:

- operator request;
- essential service failure;
- startup failure;
- shutdown deadline;
- parent scope cancellation;
- test harness teardown.

Do not expose arbitrary user text as a reason field.

## Phase E: supervisor implementation

### Ownership

The supervisor owns every long-lived service task and every supervisor-created child task.

Use a task collection such as `JoinSet` or an equivalent owned structure. Every task must be:

- awaited to completion;
- explicitly aborted after a recorded deadline; or
- transferred to another documented owner.

Discarded join handles and detached `tokio::spawn` calls are prohibited.

### Startup

Startup must:

1. validate the complete graph;
2. initialize shared runtime state;
3. start services only after required dependencies are ready;
4. enforce startup and readiness deadlines;
5. expose a deterministic router readiness result;
6. cancel already-started services when later startup fails;
7. collect all started tasks before returning failure.

Define whether independent services start concurrently. If concurrency is used, deterministic tests must not assume an unspecified poll order.

### Readiness

A service must explicitly signal readiness. Task creation alone is not readiness.

Readiness signaling must be one-shot per service instance. Duplicate readiness signals should be ignored or rejected deterministically. A service that exits before readiness is a startup failure.

### Health

Health updates must be bounded and latest-state oriented. The initial model should prefer a watch/snapshot mechanism over an unbounded event history.

A health snapshot should include:

- service identifier;
- lifecycle phase;
- healthy/degraded/failed classification;
- restart count;
- last static failure category;
- monotonic transition time supplied by the runtime clock.

Do not include raw errors, payloads, peer data, addresses, or secrets.

### Restart policy

Restartable services require an explicit policy with:

- maximum attempts in a bounded window or lifetime;
- initial delay;
- maximum delay;
- deterministic backoff rule;
- optional deterministic jitter input;
- reset conditions after sustained readiness;
- behavior after exhaustion.

The implementation must prevent hot restart loops. A restart delay of zero is invalid unless a test-only policy explicitly demonstrates bounded execution.

A restarted service receives a fresh child cancellation token and fresh task instance. Old handles and resources must be closed before the replacement is marked ready.

Do not restart essential, degradable, or optional services unless a later plan explicitly changes the classification semantics.

### Class-specific failure behavior

- Essential failure: mark failed, cancel the graph, begin shutdown, return a router failure result.
- Restartable failure: attempt policy-controlled restart; exhaustion degrades or shuts down according to an explicit registration field, not an implicit default.
- Degradable failure: mark degraded/failed and continue healthy required services.
- Optional failure: record stopped/failed state without changing router readiness after initial startup, unless the service was required for startup by another service.

Dependency failure propagation must be explicit. A service may not remain “ready” when a hard dependency is permanently unavailable.

### Panic handling

A panic in a service task must be captured as a task failure category. The panic payload must not be formatted into normal logs or health snapshots. Tests may assert classification, not payload disclosure.

The workspace remains `panic = "unwind"` for this milestone so the supervisor can observe task panics. Do not add `catch_unwind` throughout protocol code.

## Phase F: graceful and forced shutdown

Implement a two-stage shutdown:

1. graceful cancellation and join within a configured deadline;
2. forced abort of remaining tasks, followed by joining aborted handles.

Required behavior:

- shutdown is idempotent;
- all services receive cancellation;
- services may declare a bounded graceful period;
- dependency-aware reverse ordering may be used where it provides real cleanup value;
- forced abort is recorded per service;
- the supervisor does not return until every owned task has been joined or a fatal runtime invariant is reported;
- repeated shutdown requests do not reset or extend the original deadline without explicit policy;
- resources and channels owned by the supervisor are dropped after task termination.

The final shutdown report must distinguish fully graceful, partially forced, and failed cleanup.

## Phase G: scoped child tasks

Provide a narrow child-task scope for services that need internal concurrent work in later milestones.

Requirements:

- the parent service owns the scope;
- child tasks inherit cancellation;
- child failure policy is explicit: fail parent, degrade parent, or collect result;
- the parent cannot report stopped while children remain alive;
- dropping the scope initiates cancellation and joins/aborts children;
- no general global spawn API is exposed.

Synthetic services should exercise this API, but no transport reader/writer tasks are added yet.

## Testing matrix

### Graph tests

- empty graph policy;
- deterministic topological order;
- duplicate identifier rejection;
- missing dependency rejection;
- cycle rejection;
- service-count limit;
- dependency startup sequencing;
- dependent cancellation after startup failure.

### Cancellation tests

- cancel before wait;
- cancel during wait;
- many simultaneous waiters;
- parent-to-child propagation;
- child isolation;
- first reason wins;
- repeated cancellation;
- cancellation racing with command completion and deadline.

### Supervisor tests

- all services become ready;
- service exits before readiness;
- readiness timeout;
- essential failure shuts down graph;
- restartable service recovers;
- restartable service exhausts budget;
- restart backoff cannot hot-loop;
- degradable failure preserves essential service;
- optional failure does not degrade router;
- panic classification redacts payload;
- graceful shutdown;
- forced shutdown of a noncooperative service;
- child task failure and cleanup;
- no tasks remain after supervisor completion.

Tests must use deterministic time controls from the runtime or testkit. Do not use wall-clock sleeps.

## Documentation updates

Update:

- `docs/architecture.md` with the runtime crate and task ownership model;
- `docs/security-model.md` with panic, task-leak, cancellation, and shutdown threats;
- `AGENTS.md` with no-detached-task rules;
- `CONTRIBUTING.md` with runtime test commands;
- dependency-direction script;
- root README project status;
- a Plan 021 closure record.

The protocol support matrix should remain unchanged except for wording clarifying that runtime infrastructure is not protocol support.

## Validation commands

Run at minimum:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
```

Also run deterministic supervisor tests repeatedly enough to expose scheduling assumptions. Record the exact repetition count and seed where applicable.

## Acceptance criteria

Plan 021 is complete only when:

- runtime selection and supervision policy have accepted ADRs;
- graph validation occurs before task startup;
- all long-lived tasks have an owner;
- wakeable cancellation has no polling or lost-wakeup requirement;
- readiness is explicit;
- health snapshots are bounded and privacy-safe;
- all four service classifications have tested behavior;
- restart loops are bounded;
- graceful and forced shutdown join every task;
- child-task scopes cannot leak;
- no live networking or protocol behavior was added;
- normal CI and MSRV pass;
- `plans/021-closure.md` records APIs, tests, limits, and deviations.

## Stop conditions

Stop and report if:

- the proposed API requires a global mutable router context;
- protocol crates would need Tokio dependencies;
- cancellation correctness depends on periodic polling;
- a service can outlive its supervisor without explicit ownership transfer;
- restart ordering requires undocumented scheduler behavior;
- shutdown cannot prove all tasks were joined or aborted;
- the selected async dependency fails MSRV or dependency review;
- tests require real sleeps to pass.

## Handoff

The handoff must include the final crate graph, ADRs, service registration API, lifecycle table, cancellation semantics, restart policy, shutdown report shape, task-leak evidence, exact commands/results, CI run, and unresolved questions for Plan 022.