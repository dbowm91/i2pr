# Milestone 2 Plan B: bounded communication, backpressure, and resource governance

## Purpose

Implement the bounded asynchronous communication and router-wide admission-control primitives required by every later service. This plan builds on the supervisor from Plan 021 and makes queue capacity, send behavior, deadlines, cancellation, and resource ownership explicit.

The result must make overload behavior deterministic and testable before live transports introduce attacker-controlled concurrency.

## Preconditions

- Plan 021 is complete and has a closure record.
- Service identifiers, wakeable cancellation, runtime clock, supervisor ownership, and shutdown semantics are stable enough for consumption.
- No live networking has been introduced.

## Scope

This plan may:

- add bounded command, event, request, and latest-state channel facilities to `i2pr-runtime`;
- refine resource classes and lease behavior in `i2pr-core`;
- add runtime integration for deadlines and cancellation;
- add snapshots and privacy-safe queue/resource diagnostics;
- add synthetic overload and cleanup tests;
- update architecture, security, agent, and contributor documentation.

This plan must not add transport queues, peer-specific policies, NetDB queries, tunnel admission, destination streams, or application listeners. It provides infrastructure only.

## Design principles

- Every queue capacity is explicit and nonzero.
- Every enqueue operation has a documented overload outcome.
- Waiting is always bounded by cancellation, deadline, or both.
- Queueing does not substitute for resource admission.
- Resource leases use ownership and drop semantics rather than distributed manual counters.
- The implementation does not hide unbounded retry loops behind convenience methods.
- Generic wrappers are acceptable only when they preserve concrete semantics at call sites.

## Phase A: communication inventory and taxonomy

Define four distinct communication patterns.

### Commands

Commands are point-to-point instructions sent to one service owner. They require:

- bounded queue capacity;
- ordered delivery for accepted commands;
- explicit sender-closed and receiver-closed outcomes;
- cancellation-aware and deadline-aware send;
- no silent dropping.

### Requests

Requests are commands with one bounded response path. They require:

- one response or an explicit closure/error;
- request deadline and cancellation propagation;
- response sender drop interpreted as service failure or cancellation, not an infinite wait;
- no leaked waiter when the request future is dropped.

### Events

Events are observations that may have multiple consumers. The initial API must avoid implying lossless broadcast when consumers can lag.

Each event stream must choose and document one policy:

- bounded single-consumer delivery;
- bounded broadcast with explicit lag notification;
- drop-newest with a counter;
- drop-oldest with a counter;
- coalesced latest-value delivery.

Do not provide a default policy that silently discards data.

### Latest-state snapshots

Health, readiness, reachability, and configuration snapshots should use a latest-value/watch model where historical delivery is unnecessary. Consumers must be able to detect initial absence, current value, closure, and version change.

## Phase B: bounded channel API

Implement narrowly named constructors and handles rather than exposing raw unbounded channels.

Required metadata:

- static channel name or bounded identifier;
- owner service identifier;
- capacity;
- communication class;
- overflow policy;
- optional resource class charged per queued item or byte estimate.

### Capacity policy

Define repository-wide hard ceilings for infrastructure channels. Individual service plans may select lower values.

Reject:

- zero capacity;
- capacity above the hard ceiling;
- arithmetic overflow in byte or item accounting;
- dynamic names that would create unbounded metric cardinality.

Capacity must be visible in debug/snapshot output, while queued payloads remain hidden.

### Send outcomes

Use typed outcomes or errors distinguishing:

- accepted;
- full/rejected without waiting;
- deadline elapsed;
- cancellation requested;
- receiver closed;
- resource admission denied;
- payload estimate exceeds per-item limit.

Do not collapse all overload and closure states into a generic string error.

### Send modes

Provide only concrete modes required by later services:

- `try_send`: immediate acceptance or typed rejection;
- `send_until`: wait for capacity until deadline or cancellation;
- `send_request`: enqueue a request and await its response under the same cancellation/deadline scope.

A convenience `send` with no deadline is prohibited for service-to-service traffic unless the receiver lifetime statically bounds the wait and the exception is documented.

### Receive behavior

Receivers must support cancellation-aware waiting. Closing a receiver must wake blocked senders. Dropping the last sender must wake receivers with a closed result.

The API must not require periodic polling.

## Phase C: backpressure and overflow policy

Document a decision table for each communication class.

Initial recommended policy:

| Class | Default behavior |
| --- | --- |
| control command | wait until explicit deadline, then reject |
| shutdown command | out-of-band cancellation; never queued behind ordinary work |
| request | wait until explicit deadline, response bounded by same scope |
| critical event | bounded delivery with lag/failure surfaced |
| diagnostic event | drop-newest or coalesce with explicit counter |
| health/readiness | latest-state watch |

The implementation must prevent ordinary queue saturation from blocking cancellation or supervisor shutdown.

Do not add a universal priority queue in this milestone. If shutdown or essential control must bypass normal commands, use a separate cancellation/control path with narrowly defined semantics.

## Phase D: resource governor refinement

The existing `i2pr-core::ResourceBudget` and lease ownership are the foundation. Refine them only where concrete Milestone 2 tests require stronger invariants.

### Resource classes

Maintain typed classes suitable for later expansion, including at least conceptual categories for:

- service tasks;
- child tasks;
- command queue items;
- event queue items;
- buffered bytes;
- simulated stream links;
- simulated datagram links;
- pending timers;
- test peers.

Do not add every future transport or tunnel class now. Add named future classes only when required by a concrete plan or keep an extensible bounded identifier representation.

### Lease semantics

A lease must:

- own one exact resource grant;
- release exactly once on drop;
- be non-cloneable;
- support explicit early release where useful;
- remain safe when unwinding after panic;
- not expose mutable access to global counters;
- record class and amount without embedding dynamic peer data.

### Atomic bundles

Add atomic multi-class acquisition if later operations would otherwise acquire partial resources and risk deadlock or leak.

A bundle request must:

- validate all amounts first;
- apply all-or-nothing accounting;
- use deterministic class ordering;
- return a single owned bundle or typed denial;
- release all grants together or through safely separable owned leases;
- reject duplicate classes or combine them deterministically.

Do not add an asynchronous waiter queue unless a concrete test proves nonblocking admission is insufficient. The default resource policy for this milestone is immediate grant or denial; callers may retry only under explicit bounded supervisor/service policy.

### Limit changes

Decide whether limits are immutable for one runtime instance or can be adjusted through a validated snapshot. Prefer immutable limits for Milestone 2.

If live changes are implemented, define behavior when current usage exceeds the new limit. Never revoke in-use leases silently.

## Phase E: integration between queues and resources

Provide a concrete way to charge queued work against resource budgets.

Requirements:

- resource admission occurs before payload ownership enters the queue;
- a rejected send retains payload ownership with the caller where practical;
- the queue item owns its lease while enqueued and while being processed, according to an explicit handoff rule;
- dropping a queued item releases its lease;
- receiver shutdown drains or drops items deterministically and releases every lease;
- timeout/cancellation while waiting for capacity does not retain a lease indefinitely;
- byte estimates are caller-provided, validated, and capped; they are policy estimates, not proof of allocator usage.

Avoid a design where channels reach into a global service locator to acquire budgets.

## Phase F: snapshots and diagnostics

Expose bounded snapshots for tests and operator-facing status later.

Channel snapshot fields:

- static identifier;
- class;
- capacity;
- current queued item count;
- accepted count;
- rejected-full count;
- deadline count;
- cancellation count;
- closed count;
- dropped/coalesced count where applicable.

Resource snapshot fields:

- class;
- configured limit;
- current usage;
- high-water mark;
- denied count.

Counters must use checked or saturating behavior with a documented overflow policy. Snapshotting must not block the runtime for unbounded time.

Do not expose payload contents, secrets, peer identities, addresses, destination hashes, or unbounded labels.

## Phase G: synthetic service integration

Build a non-networked synthetic graph using Plan 021 services:

- one producer service;
- one bounded worker service;
- one health/snapshot observer;
- one intentionally slow or blocked consumer;
- router-wide resource limits below offered load.

Demonstrate:

- accepted commands preserve order;
- overload is rejected or times out as configured;
- cancellation bypasses saturation;
- requests do not leak waiters;
- queue shutdown releases leases;
- resource high-water marks are reproducible;
- supervisor shutdown leaves zero usage and zero live tasks.

## Testing matrix

### Channel construction

- zero capacity rejected;
- excessive capacity rejected;
- stable identifier validation;
- metadata snapshots redact payloads.

### Commands and requests

- immediate success;
- immediate full rejection;
- wait then success after capacity opens;
- deadline before capacity;
- cancellation before send;
- cancellation during wait;
- receiver closes while sender waits;
- sender closes while receiver waits;
- request response succeeds;
- response sender drops;
- requester drops before response;
- shutdown under full queue.

### Event policies

- lag is surfaced for bounded broadcast;
- drop counters are exact;
- latest-state consumers observe newest version;
- slow consumer cannot grow memory without bound;
- event closure wakes consumers.

### Resource accounting

- exact limit accepted;
- maximum-plus-one denied;
- zero amount policy;
- arithmetic overflow rejected;
- lease drop releases usage;
- explicit release is idempotent or consumes the lease;
- panic unwinding releases lease;
- atomic bundle success;
- atomic bundle denial leaves usage unchanged;
- duplicate bundle classes handled deterministically;
- concurrent acquisitions never exceed limit;
- channel item drop releases charged resources;
- forced supervisor shutdown returns all usage to zero.

Use deterministic runtime time. Do not use wall-clock sleeps.

## Documentation and closure

Update:

- `docs/architecture.md` with communication and resource ownership;
- `docs/security-model.md` with queue exhaustion, slow consumer, waiter leak, and accounting threats;
- `AGENTS.md` with explicit queue/deadline rules;
- `CONTRIBUTING.md` with overload test guidance;
- root README current status;
- Plan 022 closure record.

No protocol-support status changes are warranted.

## Validation commands

Run the standard workspace, MSRV, dependency-direction, dependency-policy, Clippy, rustdoc, and test gates. Additionally:

- repeat concurrent accounting tests under multiple deterministic seeds;
- run synthetic overload tests with capacities 1, exact load, and maximum-plus-one load;
- verify all snapshots return zero live usage after teardown;
- inspect source for raw `unbounded_channel`, discarded send results, and sends lacking deadline/cancellation policy.

A repository script that rejects production uses of unbounded Tokio channels is recommended if it can avoid false confidence and false positives.

## Acceptance criteria

Plan 022 is complete only when:

- every runtime communication primitive is bounded;
- send and receive waits are cancellation/deadline aware;
- overload outcomes are typed and tested;
- shutdown cannot be blocked behind ordinary queue traffic;
- request cancellation cannot leak response waiters;
- resource leases are non-cloneable and release on every exit path;
- atomic bundle denial cannot leave partial usage;
- queue-held work owns and releases its resource charge;
- snapshots are bounded and privacy-safe;
- synthetic overload returns task count, queue depth, and resource usage to zero;
- no live networking or protocol policy was added;
- normal CI and MSRV pass;
- `plans/022-closure.md` records capacities, policies, tests, and deviations.

## Stop conditions

Stop and report if:

- an API requires an unbounded queue;
- cancellation can be starved by a full command queue;
- resource admission requires holding a lock across `.await`;
- multi-resource acquisition can partially succeed;
- queue metrics require dynamic peer-derived labels;
- correctness depends on unspecified Tokio fairness;
- tests need real sleeps;
- a generic abstraction obscures overflow or ownership behavior at call sites.

## Handoff

The handoff must include the channel taxonomy, capacity ceilings, overload decision table, send/receive result types, resource classes, bundle semantics, snapshot fields, synthetic graph configuration, deterministic test results, CI run, and open questions for Plan 023.