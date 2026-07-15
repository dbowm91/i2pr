# Milestone 2 Plan D: privacy-aware observability, integrated validation, and closure

## Purpose

Complete Milestone 2 by defining safe runtime observability, validating the complete supervised simulation stack, documenting failure and cleanup behavior, and producing an aggregate closure record suitable for NTCP2 handoff.

This plan does not add protocol behavior. Its job is to prove that the runtime foundation remains bounded, diagnosable, deterministic, and clean under ordinary and adversarial synthetic conditions.

## Preconditions

- Plan 021 supervision and cancellation are complete.
- Plan 022 bounded communication and resource governance are complete.
- Plan 023 deterministic clock, links, scheduler, and fault injection are complete.
- Each plan has an individual closure record with green normal and MSRV CI evidence.
- No production crate depends on `i2pr-testkit`.

## Scope

This plan may:

- define structured tracing conventions and safe runtime event fields;
- add bounded runtime snapshots and diagnostic summaries;
- add integrated deterministic service-graph scenarios;
- add leak, overload, restart, and shutdown regression tests;
- add CI lanes or scripts for deterministic simulation checks;
- update architecture, security, README, agent, and contributor documentation;
- create the aggregate Milestone 2 closure record.

This plan must not:

- open network sockets;
- add transport or peer protocol behavior;
- expose an administrative listener;
- persist detailed runtime event histories by default;
- add high-cardinality peer-derived metric labels;
- claim anonymity, resilience, or interoperability.

## Phase A: observability threat model

Document the information that runtime diagnostics must not expose by default:

- private keys or session keys;
- reply keys or session tags;
- raw protocol payloads;
- RouterIdentity or Destination encodings;
- full identity/destination hashes;
- peer IP addresses or hostnames;
- local filesystem paths containing user-specific information, except where a CLI error must identify an explicitly supplied path;
- arbitrary attacker-controlled strings;
- precise per-peer timing histories;
- unbounded panic or error payloads.

Define safe categories:

- static service identifier;
- static channel identifier;
- lifecycle phase;
- bounded failure category;
- service classification;
- restart attempt count;
- queue capacity and current depth;
- resource class and aggregate usage;
- monotonic duration or deadline class;
- simulation link identifier and sequence number;
- deterministic scenario identifier and seed.

Observability is not a justification to retain sensitive data that the runtime does not otherwise need.

## Phase B: structured tracing conventions

Define a stable event naming scheme, for example:

```text
runtime.service.registered
runtime.service.starting
runtime.service.ready
runtime.service.degraded
runtime.service.failed
runtime.service.restarting
runtime.service.stopping
runtime.service.stopped
runtime.shutdown.requested
runtime.shutdown.forced
runtime.channel.rejected
runtime.resource.denied
simulation.fault.applied
simulation.completed
```

Exact names may differ, but they must be documented and bounded.

### Field rules

- Use static field keys.
- Prefer enums or numeric counters over free-form messages.
- Do not record raw `Debug` output from arbitrary service errors.
- Do not record panic payload text.
- Do not use peer IDs, destination hashes, addresses, or dynamic request IDs as metric labels.
- Event fields representing counts or sizes must have units in their names or documentation.
- Durations must be monotonic and avoid wall-clock correlation where unnecessary.
- Simulation seeds may be logged in test/research contexts because they are reproducibility inputs, not cryptographic secrets.

### Subscriber boundary

`i2pr-daemon` remains responsible for subscriber configuration. Lower crates may emit structured events through `tracing`, but must not install global subscribers.

Repeated initialization in tests should remain harmless. The plan should clarify production initialization, test capture, and redaction ownership.

## Phase C: bounded health and runtime snapshots

Provide a coherent router runtime snapshot assembled from the supervisor, channels, resources, clock, and simulation harness.

Required sections:

- router lifecycle and readiness;
- service states and restart counts;
- channel capacities/depths and aggregate outcome counters;
- resource limits, usage, high-water marks, and denials;
- child-task counts;
- pending timer count;
- simulated link and pending-delivery counts in test builds;
- shutdown status and forced-abort count.

Requirements:

- snapshots are bounded by configured service/channel/resource maxima;
- snapshot generation has bounded work;
- snapshots do not lock mutable runtime state across `.await`;
- inconsistent partial reads are either versioned or documented as eventually coherent;
- raw payloads and sensitive identifiers are absent;
- default `Debug` output remains safe;
- test-only simulation details do not leak into production interfaces unless they are generic aggregate counters.

Do not build a web API, persistent database, Prometheus endpoint, or metrics exporter in this milestone.

## Phase D: integrated deterministic scenarios

Create a small set of named, reproducible integration scenarios.

### Scenario 1: clean startup and shutdown

Topology:

- essential coordinator;
- restartable worker;
- degradable observer;
- optional reporter;
- bounded command and latest-state channels;
- resource limits comfortably above load.

Prove:

- dependency-ordered readiness;
- stable snapshot;
- graceful cancellation;
- no forced abort;
- zero final tasks, queues, timers, links, and resource usage.

### Scenario 2: bounded overload

Use queue capacities and resource limits below offered synthetic load.

Prove:

- accepted work stays within bounds;
- excess work receives typed full/deadline/resource denial;
- shutdown remains responsive;
- no waiter or lease leak;
- high-water marks match configured ceilings;
- replay is identical for the same seed.

### Scenario 3: restart and recovery

Inject deterministic restartable-service failures.

Prove:

- restart count and backoff are correct;
- old child tasks and channels close before replacement readiness;
- recovery occurs within budget;
- another seed or script can exhaust the budget deterministically;
- exhaustion follows the configured router policy.

### Scenario 4: essential failure

Inject failure after all services are ready.

Prove:

- failure category is recorded without payload disclosure;
- coordinated cancellation begins once;
- dependents stop;
- graceful deadline is enforced;
- a deliberately noncooperative optional service is force-aborted;
- final shutdown report distinguishes graceful and forced services;
- all owned tasks are joined.

### Scenario 5: simulated link faults

Use stream and datagram links with delay, duplication, reordering, truncation, drop, and disconnect rules.

Prove:

- deterministic ordering and replay;
- pending-delivery and byte limits;
- cancellation tears down scheduler and endpoints;
- trace records include safe rule/sequence metadata only;
- no payload bytes appear in snapshots or logs.

## Phase E: concurrency and cleanup review

Perform a targeted review of every runtime ownership path.

Inventory:

- supervisor tasks;
- child task scopes;
- cancellation waiters;
- channel send waiters;
- request response waiters;
- manual-clock sleepers;
- scheduled network deliveries;
- queued stream segments;
- queued datagrams;
- resource leases and bundles;
- tracing capture buffers used by tests.

For each, document:

- owner;
- creation bound;
- cancellation path;
- graceful completion path;
- forced cleanup path;
- final invariant;
- tests covering success, error, cancellation, and panic where applicable.

Add debug-only or test-only counters only when they do not alter production behavior or require unsafe code.

### Optional model checking

Evaluate `loom` or another concurrency model-checking tool for the smallest synchronization primitives, especially resource accounting and cancellation state. Add it only if:

- the dependency is isolated to tests;
- the modeled code can be shared without maintaining a second implementation;
- the test has a clear invariant not already covered by deterministic async tests;
- runtime and MSRV policy remain intact.

A documented decision not to add model checking is acceptable. Do not add a large toolchain merely to satisfy a checklist.

## Phase F: deterministic soak matrix

Run bounded scenario matrices across fixed seeds and limits.

Minimum matrix:

- at least 32 fixed root seeds;
- channel capacities 1, 2, and a normal configured value;
- resource limits at zero where legal, one, exact demand, and demand-minus-one;
- restart outcomes: no failure, one recovery, budget exhaustion;
- shutdown outcomes: fully graceful and forced abort;
- stream and datagram fault combinations;
- maximum pending timer and delivery boundaries.

Each case must have a maximum simulated step count and maximum test execution budget. Failures must print the scenario identifier and root seed.

Do not use OS randomness in the reproducibility matrix. A separate opt-in exploratory seed runner may use OS randomness if every selected seed is printed and retained on failure.

## Phase G: CI and repository checks

Add or update repository automation as justified.

Required CI coverage:

- normal Ubuntu and macOS quality jobs;
- Rust 1.85 MSRV check;
- dependency policy;
- deterministic integrated scenarios;
- no wall-clock sleep usage in designated simulation tests where mechanically checkable;
- dependency-direction enforcement;
- documentation warnings denied.

Recommended scripts:

- a runtime-boundary check for forbidden production dependency on `i2pr-testkit`;
- a source check for unbounded Tokio channels in production crates;
- a deterministic scenario runner that prints seed and scenario IDs;
- a fixture/artifact check confirming simulations do not commit private keys, crash artifacts, or raw captures.

Scripts must be small, readable, and treated as guardrails rather than formal proofs.

## Phase H: documentation reconciliation

Update:

- `README.md` project status and explicit non-router warning;
- `docs/architecture.md` crate graph, service lifecycle, channels, resources, and simulation boundary;
- `docs/security-model.md` overload, task leak, cancellation, diagnostic leakage, and simulation limitations;
- `AGENTS.md` task ownership, queue, seed, and trace rules;
- `CONTRIBUTING.md` runtime and deterministic scenario commands;
- runtime and testkit crate-level documentation;
- ADR index and accepted decisions;
- known limitations.

Do not change protocol support statuses. Milestone 2 infrastructure is not protocol implementation evidence.

## Aggregate Milestone 2 closure record

Create `plans/020-milestone-2-closure.md` after all work is complete.

The record must include:

- Plan 021–024 implementation and closure commits;
- final crate dependency graph;
- runtime and cancellation ADRs;
- service classification and lifecycle table;
- graph and startup semantics;
- restart policy;
- graceful/forced shutdown behavior;
- channel taxonomy and capacity ceilings;
- resource classes and lease/bundle ownership;
- clock and seed-derivation contracts;
- stream/datagram semantics;
- fault rule composition and replay format;
- observability field allowlist and sensitive-field denylist;
- integrated scenario inventory;
- soak matrix and regression seeds;
- task/channel/timer/link/resource final invariants;
- exact local commands and results;
- CI run links;
- dependency additions and MSRV review;
- deviations and known limitations;
- explicit Milestone 3 prerequisites.

## Milestone 2 validation matrix

Run at minimum:

```text
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
git diff --check
```

Also run:

- the complete deterministic scenario matrix;
- repeated same-seed replay comparison;
- overload and forced-shutdown scenarios;
- source checks for detached tasks, unbounded channels, production testkit dependencies, and unsafe code;
- targeted tracing capture tests confirming sensitive values and panic payloads are absent;
- teardown assertions confirming zero tasks, waiters, timers, queued deliveries, links, and resource usage;
- Linux and macOS CI.

Record exact versions, seed list, step limits, and any environment-specific exclusions.

## Milestone 2 acceptance criteria

Milestone 2 closes only when:

- all Plan 021–024 individual closure records exist;
- the aggregate Milestone 2 closure record exists;
- every long-lived task and waiter has a documented owner and cleanup path;
- essential, restartable, degradable, and optional behavior is integrated and tested;
- cancellation is wakeable and hierarchical;
- every queue is bounded and overload behavior is explicit;
- resource accounting returns to zero on every terminal path;
- deterministic clock and seed-domain separation are demonstrated;
- stream/datagram faults replay identically from the same seed;
- snapshots and tracing obey the privacy allowlist;
- integrated scenarios complete without wall-clock sleeps;
- normal, MSRV, dependency, documentation, and deterministic CI gates pass;
- no socket, peer exchange, NetDB behavior, tunnel, client API, or capability advertisement exists.

## Milestone 3 readiness gate

Before NTCP2 planning or implementation begins, the closure must prove:

- a transport service can be represented as a supervised essential or restartable service without changing supervisor contracts;
- transport reader/writer child tasks can be owned by a service scope;
- handshake and link queues can use bounded channels and resource leases;
- transport timeouts can consume the clock contract;
- replay protection and test handshakes can use domain-separated test RNGs;
- in-memory stream links can model partial I/O, delay, truncation, disconnect, and backpressure;
- failure and shutdown cannot leak a link task or buffered bytes;
- tracing can report transport categories without peer-derived labels.

Do not create NTCP2 sockets or handshake code as part of proving this gate.

## Stop conditions

Stop and report if:

- integrated tests reveal nondeterminism under the same seed;
- final snapshots cannot prove zero owned tasks or resources;
- observability requires raw identities, addresses, payloads, or panic text;
- shutdown relies on wall-clock sleeps;
- CI passes only after weakening overload or cleanup assertions;
- a production dependency on `i2pr-testkit` appears;
- a runtime API must be redesigned to accommodate transport before Milestone 3 planning;
- soak scenarios can livelock without a bounded step guard.

## Handoff

The final handoff must summarize the complete runtime architecture, privacy review, integrated scenarios, seed matrix, leak invariants, commands/results, CI evidence, known limitations, and the exact commit establishing Milestone 2 closure.