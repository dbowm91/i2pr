# Milestone 2 Plan C: deterministic clock, in-memory links, and fault injection

## Purpose

Turn `i2pr-testkit` from a vocabulary crate into a deterministic simulation environment suitable for transport and router state-machine development. This plan provides controllable monotonic time, reproducible randomness, bounded in-memory stream and datagram links, deterministic fault injection, and test peer factories.

The testkit must remain outside the production dependency graph and must not become a backdoor for unrestricted production hooks.

## Preconditions

- Plan 021 supervision and cancellation are complete.
- Plan 022 bounded channels and resource accounting are complete.
- Runtime clock and cancellation contracts are stable.
- The Milestone 1 identity factory can create ephemeral deterministic identities without committing private fixtures.

## Scope

This plan may:

- extend `i2pr-testkit` substantially;
- add test-only dependencies on `i2pr-runtime`, `i2pr-core`, `i2pr-proto`, and `i2pr-crypto` as needed;
- refine narrow clock contracts in `i2pr-core` or `i2pr-runtime`;
- implement bounded simulated stream and datagram endpoints;
- implement deterministic network scheduling and fault scripts;
- provide ephemeral test peers, identities, RouterInfo values, and topology builders;
- add simulation replay records and deterministic integration tests.

This plan must not:

- open real sockets;
- perform DNS resolution;
- connect to the public I2P network;
- persist generated private identities in fixtures;
- expose fault injection through production router configuration;
- claim transport interoperability.

## Design principles

- One seed plus one topology and fault script must reproduce the same result.
- Time advances under test control; no test sleeps for real time.
- Queue and buffer capacity remain explicit.
- Stream and datagram semantics remain distinct.
- Faults operate on documented units and deterministic sequence numbers.
- Simulation traces are bounded and privacy-safe.
- The testkit models failure, not unspecified scheduler luck.

## Phase A: deterministic time contract

### Monotonic instant

Provide a runtime-neutral monotonic instant representation suitable for:

- deadlines;
- ordering;
- duration calculation;
- serialization into deterministic test reports;
- overflow-safe arithmetic.

Do not use wall-clock timestamps for protocol timeout logic.

### Clock interface

Implement the narrowest clock contract required by services and simulations. It must support:

- `now`;
- checked deadline calculation;
- asynchronous sleep until a monotonic deadline;
- cancellation-aware waiting at call sites;
- production Tokio-backed implementation;
- manual deterministic implementation.

Avoid `async-trait` if an associated future or another stable, narrow approach remains readable under Rust 1.85. Do not create a generalized timer-wheel or runtime abstraction.

### Manual clock

Upgrade the existing `ManualClock` so advancing time wakes all due sleepers.

Required behavior:

- sleep registered before advance wakes exactly when due;
- sleep registered after the deadline completes immediately;
- multiple sleepers with the same deadline wake without lost notifications;
- advancing by zero is deterministic;
- advancing past multiple deadlines releases them in stable deadline/sequence order;
- overflow is rejected;
- cancelled sleep removes or harmlessly invalidates its waiter;
- dropping all clock handles wakes or fails pending sleepers deterministically during teardown;
- no wall-clock fallback exists.

Keep the number of pending timers bounded through the resource governor or an explicit testkit limit.

## Phase B: reproducible randomness

Retain `ReproducibilitySeed` and `DeterministicRng`, then add deterministic domain separation.

A root seed should derive independent child seeds for:

- topology generation;
- each simulated link;
- each direction of a link;
- fault selection;
- service restart jitter;
- test identity generation;
- message generation.

Use a documented deterministic derivation method based on a reviewed hash or keyed construction. Do not reuse one mutable RNG across unrelated concurrent components, because task interleaving would then change results.

Each simulation failure report must print the root seed and stable scenario identifier. It must not print generated private keys.

Tests must prove:

- same root seed and labels produce the same child seeds;
- different domain labels produce distinct streams;
- component execution order does not alter another component's random sequence;
- seed display/parse round trips.

## Phase C: deterministic network scheduler

Implement a bounded scheduler that owns pending simulated deliveries.

Each scheduled unit should contain:

- link identifier;
- direction;
- monotonically increasing sequence number;
- delivery deadline;
- stream segment or datagram payload;
- applied fault metadata safe for diagnostics;
- resource lease for queued item and buffered bytes.

Ordering must be deterministic by a documented tuple, such as:

```text
(delivery_deadline, link_id, direction, sequence_number, duplicate_index)
```

Do not depend on hash-map order or Tokio poll order.

Scheduler requirements:

- explicit maximum pending deliveries;
- explicit maximum buffered bytes;
- cancellation wakes the scheduler;
- advancing manual time delivers all due units;
- receiver backpressure is respected;
- teardown drops pending units and releases all leases;
- no scheduler task survives simulation shutdown.

## Phase D: in-memory stream links

Implement bounded duplex byte-stream endpoints suitable for future NTCP2 state-machine tests.

Required semantics:

- ordered bytes in the absence of injected faults;
- partial writes and partial reads;
- configurable send and receive capacities;
- half-close and full disconnect behavior;
- writer observes closed peer;
- reader observes EOF only after queued bytes are drained, unless a reset fault explicitly discards them;
- cancellation-aware read and write operations;
- write deadlines and read deadlines;
- no hidden unbounded coalescing;
- deterministic segmentation independent of consumer read size where specified.

The first API may be testkit-specific instead of implementing Tokio `AsyncRead`/`AsyncWrite`. Add those traits only if Milestone 3 will consume them directly and the adapter preserves fault and accounting semantics.

Define the fault unit for streams. Recommended initial unit: a scheduled write segment with a stable sequence number, not arbitrary individual bytes.

## Phase E: in-memory datagram links

Implement bounded duplex datagram endpoints suitable for future SSU2 tests.

Required semantics:

- datagram boundaries preserved;
- explicit maximum datagram size;
- send queue and receive queue capacities;
- source endpoint identity represented by a synthetic bounded address, not a real IP;
- receive returns one complete datagram or a typed truncation result according to API policy;
- cancellation-aware send and receive;
- deadline-aware send and receive;
- disconnect/closed behavior;
- resource accounting for queued datagrams and bytes.

Do not simulate UDP checksum, kernel socket buffers, MTU discovery, NAT, IPv4, or IPv6 behavior in this milestone. Those require explicit later plans.

## Phase F: fault scripts

Convert the existing `FaultAction` vocabulary into deterministic executable scripts.

Supported actions:

- drop;
- delay;
- duplicate;
- reorder within a bounded window;
- truncate;
- disconnect/reset.

### Matching

A fault rule should match a bounded set of fields such as:

- link identifier;
- direction;
- stream or datagram kind;
- exact sequence number;
- bounded sequence range;
- every Nth unit under an explicit maximum;
- deterministic probability using a dedicated fault RNG.

Avoid matching arbitrary payload contents in the initial API. Payload-aware mutation belongs to protocol fuzzing, not generic link simulation.

### Fault semantics

Define exactly:

- whether delay is additive or absolute;
- how duplicates receive sequence/duplicate indices;
- how reorder windows flush;
- whether truncation of a stream segment preserves remaining bytes or discards them;
- whether datagram truncation returns a shorter datagram or an explicit error;
- whether disconnect is graceful EOF or reset;
- how multiple matching rules compose;
- maximum rules per link and maximum generated duplicate units.

Reject scripts that can create unbounded expansion, zero-progress loops, or time overflow.

### Replay record

Produce a bounded replay record containing:

- root seed;
- scenario identifier;
- link/topology summary;
- applied fault rule identifiers;
- sequence numbers and safe outcome categories;
- final simulated time;
- final task, queue, timer, and resource snapshots.

Do not include raw payloads, private identities, destination hashes, real addresses, or full RouterInfo bytes by default.

## Phase G: test peer and topology factories

Provide deterministic ephemeral factories for:

- router identities using injected deterministic RNG;
- no-capability RouterInfo records;
- synthetic service identifiers;
- synthetic peer identifiers distinct from real RouterIdentity hashes where a real identity is unnecessary;
- stream endpoint pairs;
- datagram endpoint pairs;
- linear, star, ring, and small arbitrary test topologies.

Factories must:

- enforce explicit maximum peer counts;
- use domain-separated seeds;
- generate no files by default;
- expose public summaries only;
- avoid pretending a generated RouterInfo is network-valid or interoperable;
- preserve exact reproducibility.

Private keys must remain memory-only and zeroizing. Do not return them in debug snapshots.

## Phase H: deterministic simulation harness

Create a harness that composes:

- Plan 021 supervisor;
- Plan 022 bounded channels and resource budget;
- manual clock;
- domain-separated RNGs;
- network scheduler;
- stream/datagram links;
- fault scripts;
- synthetic services and peers.

The harness must support:

- start;
- advance to next scheduled event;
- advance by duration;
- run until idle under a maximum step count;
- run until predicate/deadline;
- inject cancellation;
- collect bounded snapshots;
- shut down and assert no leaks.

`run_until_idle` must reject or stop when work continually reschedules itself beyond an explicit step limit. It must not hang.

## Testing matrix

### Clock

- immediate expired sleep;
- one sleeper;
- many equal-deadline sleepers;
- multiple ordered deadlines;
- cancelled sleeper;
- clock teardown;
- overflow;
- no wall-clock elapsed-time dependency.

### RNG

- seed parse/display;
- domain separation;
- order independence;
- same-seed replay;
- different-seed divergence.

### Scheduler

- deterministic ordering;
- exact-time delivery;
- pending-item limit;
- byte limit;
- receiver backpressure;
- cancellation and teardown;
- resource usage returns to zero.

### Streams

- ordered byte delivery;
- partial read/write;
- queue saturation;
- half-close;
- reset;
- read/write deadline;
- cancellation;
- delayed delivery;
- duplicate segment;
- reorder window;
- truncation;
- reproducible combined faults.

### Datagrams

- boundary preservation;
- maximum and maximum-plus-one size;
- queue saturation;
- drop;
- duplicate;
- delay;
- reorder;
- truncate;
- disconnect;
- deterministic source identity.

### Harness

- small graph reaches idle;
- step limit catches livelock;
- same seed produces identical replay record;
- different seed changes fault outcome where configured;
- cancellation tears down graph;
- final task/channel/timer/resource snapshots are zero.

Run tests across multiple deterministic seed values, including zero, all-ones, and several fixed regression seeds. Do not use random OS seeds in ordinary CI tests.

## Documentation and closure

Update:

- `docs/architecture.md` with the simulation boundary;
- `docs/security-model.md` with simulation non-equivalence and test-hook risks;
- `AGENTS.md` with seed reporting and no-public-network fault testing;
- `CONTRIBUTING.md` with deterministic reproduction commands;
- root README current status;
- Plan 023 closure record.

The support ledger remains unchanged because simulation is not protocol support.

## Validation commands

Run the standard workspace, MSRV, Clippy, rustdoc, dependency, and CI gates. Also run:

- deterministic test suite under a documented seed matrix;
- repeated same-seed replay comparisons;
- simulation teardown assertions;
- source scan confirming no real socket use in `i2pr-testkit`;
- source scan confirming no private key debug or fixture output;
- bounded stress of maximum pending deliveries and maximum buffered bytes.

## Acceptance criteria

Plan 023 is complete only when:

- manual time wakes asynchronous sleepers without real sleeps;
- randomness is domain-separated and scheduling-order independent;
- stream and datagram links are bounded and semantically distinct;
- every required fault action is executable and bounded;
- same seed/topology/script yields the same replay record;
- test peer factories keep private material memory-only and redacted;
- simulation can run until idle or fail under a bounded step limit;
- shutdown leaves no live tasks, timers, queued units, or resource leases;
- no production crate depends on `i2pr-testkit`;
- no live socket or protocol support claim is introduced;
- normal CI and MSRV pass;
- `plans/023-closure.md` records API, seeds, limits, tests, and deviations.

## Stop conditions

Stop and report if:

- reproducibility depends on Tokio poll order;
- a shared mutable RNG makes component results order-dependent;
- timer wakeup requires wall-clock polling;
- fault composition can expand work without a hard bound;
- stream and datagram semantics are collapsed into one ambiguous API;
- a test factory writes private keys to disk by default;
- production code requires testkit hooks;
- simulation teardown cannot prove zero live tasks and resources.

## Handoff

The handoff must include the clock contract, seed derivation, scheduler ordering tuple, stream/datagram semantics, fault composition rules, topology factories, replay format, capacity limits, seed matrix, exact commands/results, CI run, and open questions for Plan 024.