# Milestone 2 overview: supervised runtime and deterministic network test infrastructure

## Objective

Establish the lifecycle, cancellation, communication, resource, observability, and deterministic simulation infrastructure required before implementing NTCP2 or any other live router subsystem.

Milestone 2 introduces a concrete asynchronous supervisor and testable service graph while keeping protocol/state-machine ownership outside the runtime. It must prove that long-lived work can start, report readiness, fail according to policy, apply backpressure, cancel, and shut down without leaked tasks or resources.

No live I2P networking is part of this milestone.

## Governing material

Read before implementation:

- `GUARDRAILS.md`
- `plans/000-mvp-roadmap.md`
- `plans/015-m1-corrective-closure.md`
- `plans/010-milestone-1-closure.md` once created
- `docs/architecture.md`
- `docs/security-model.md`
- relevant ADRs under `docs/adr/`
- `AGENTS.md`
- `CONTRIBUTING.md`

Milestone 2 does not implement a protocol dossier. It nevertheless remains subject to `specs/CONFORMANCE.md` where shared limits, evidence, and support wording are relevant.

## Preconditions

Milestone 2 may begin only after:

- Plan 015 is complete;
- the aggregate Milestone 1 closure record exists;
- normal and Rust 1.85 CI are green;
- no unresolved Milestone 1 secret-handling or wire-format defect affects the runtime boundary;
- the protocol support ledger remains non-advertised;
- the current architecture document accurately describes the six-crate baseline.

## Plan set and dependency order

Execute in this order:

1. `plans/021-m2-supervision-cancellation.md`
2. `plans/022-m2-bounded-channels-resource-governor.md`
3. `plans/023-m2-deterministic-network-testkit.md`
4. `plans/024-m2-observability-validation-closure.md`

Parallel work is allowed only after the contracts introduced by Plan 021 are merged. Avoid parallel edits to service lifecycle types, cancellation semantics, channel result enums, resource lease ownership, or workspace dependency policy.

## Intended workspace change

Milestone 2 may add one concrete production crate:

```text
crates/i2pr-runtime/
```

Its purpose is narrow and immediate: own Tokio-backed supervision, wakeable cancellation, bounded asynchronous communication, runtime clocks, and task ownership.

Expected dependency direction:

```text
                   i2pr-proto <- i2pr-crypto <- i2pr-storage
                        ^
                        |
                   i2pr-core
                        ^
                        |
                  i2pr-runtime
                        ^
                        |
                  i2pr-daemon

                  i2pr-testkit
                  test/simulation only
```

More precisely:

- `i2pr-core` remains runtime-neutral and owns semantic contracts, identifiers, lifecycle classifications, health vocabulary, resource classes, and snapshots.
- `i2pr-runtime` depends on `i2pr-core` and concrete async dependencies.
- `i2pr-daemon` composes the runtime and remains the process root.
- `i2pr-proto`, `i2pr-crypto`, and `i2pr-storage` must not depend on Tokio or `i2pr-runtime`.
- `i2pr-testkit` may depend on runtime crates for test-only simulation, but production crates must not depend on `i2pr-testkit`.

If implementation can remain simpler inside `i2pr-daemon` without causing future subsystem crates to depend on the daemon, stop and record the alternative in an ADR before creating `i2pr-runtime`. The default plan assumes the concrete runtime crate is justified by immediate supervisor and channel code.

## Runtime policy

Milestone 2 should use Tokio as the initial concrete runtime unless dependency or MSRV review reveals a blocker.

Requirements:

- Tokio is an implementation detail of runtime-facing crates.
- Do not introduce a project-wide generic runtime trait.
- Enable only features required by this milestone.
- Do not enable TCP or UDP features merely in anticipation of Milestone 3.
- Evaluate whether wakeable cancellation uses `tokio-util::sync::CancellationToken` or a small Tokio-native implementation; record the decision in an ADR.
- Pin behavior through tests rather than relying on undocumented scheduler ordering.
- Preserve Rust 1.85 compatibility across the production workspace.

## Milestone-wide constraints

### No live networking

Do not:

- bind TCP or UDP sockets;
- connect to peers;
- perform DNS resolution;
- reseed;
- read or publish NetDB records as network behavior;
- create network tunnels;
- expose SAM, I2CP, HTTP, SOCKS5, IRC, or administrative listeners;
- make live `i2pr run` appear functional;
- advertise transport or router capabilities.

In-memory streams and datagrams are permitted only in `i2pr-testkit` and tests.

### No detached tasks

Every spawned task must be owned by a supervisor, a scoped child group, or an explicitly awaited short-lived operation. A `tokio::spawn` whose handle is discarded is a milestone failure.

### Every queue is bounded

All asynchronous command, event, health, and simulated-link queues must have explicit nonzero capacities. Overflow and timeout behavior must be part of the type or call-site contract.

### Explicit failure policy

Every long-lived service must declare one classification:

- essential;
- restartable;
- degradable;
- optional.

The classification determines supervisor behavior and must not be inferred from service names.

### Runtime-neutral protocol code

No async runtime, channel, task, timer, or tracing-subscriber dependency may enter `i2pr-proto`, `i2pr-crypto`, or `i2pr-storage`.

### Determinism

Simulation outcomes must be reproducible from an explicit seed and deterministic clock. Tests must not depend on wall-clock sleeps or unspecified task scheduling order.

## Required deliverables

By Milestone 2 closure, the repository should contain:

- a reviewed runtime ADR and dependency decision;
- concrete `i2pr-runtime` crate or explicitly justified daemon-local alternative;
- service registration and dependency-graph validation;
- supervised task ownership and child-task scopes;
- wakeable hierarchical cancellation with recorded reasons;
- readiness, health, failure, restart, and shutdown reporting;
- bounded command/event channel facilities with explicit send outcomes;
- router-wide resource budget snapshots and scoped lease bundles;
- deterministic monotonic clock and reproducible RNG interfaces for tests;
- bounded in-memory stream and datagram links;
- deterministic fault injection for loss, delay, duplication, reordering, truncation, and disconnect;
- test peer and identity factories that do not commit private keys;
- privacy-aware tracing field conventions;
- a simulated multi-service graph demonstrating startup through forced shutdown;
- an aggregate Milestone 2 closure record.

## Intended service shape

A long-lived service should conceptually produce:

```text
registration/specification
  + command handle(s)
  + readiness/health receiver
  + owned task future or factory
```

The exact API must remain concrete. It must express:

- stable service identifier;
- service classification;
- startup dependencies;
- readiness transition;
- health updates;
- cancellation input;
- completion/failure result;
- owned resource and child-task cleanup.

Do not pass a global mutable router context into every service.

## Milestone exit criteria

Milestone 2 closes only when:

- a simulated service graph starts in dependency order and reports readiness;
- essential failure initiates coordinated shutdown;
- restartable failure follows bounded restart policy and cannot hot-loop;
- degradable and optional failures produce correct health outcomes without accidental process termination;
- cancellation wakes blocked services rather than requiring polling;
- graceful shutdown has an explicit deadline and forced-abort result;
- no spawned task survives test teardown;
- all channels are bounded and overflow behavior is tested;
- resource leases are released on success, error, timeout, panic, cancellation, and forced shutdown;
- deterministic tests use no wall-clock sleeps;
- stream and datagram faults replay identically from the same seed;
- traces and health snapshots contain no raw secrets, identities, destinations, peer addresses, or message bytes;
- normal CI, MSRV, dependency policy, documentation, and deterministic integration tests pass;
- no live network or protocol capability is introduced.

## Handoff standard

Every plan closure must include:

- changed files and crate dependencies;
- public contracts and ownership rules;
- service/task/channel/resource limits;
- cancellation and shutdown semantics;
- deterministic seeds and simulation topology;
- exact commands and results;
- CI evidence;
- failure-path tests;
- privacy review;
- deviations and unresolved questions;
- explicit prerequisites for the next plan.

A statement that a simulated service “works” without task, queue, resource, and deterministic replay evidence is insufficient.