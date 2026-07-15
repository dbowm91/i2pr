# Plan 022 closure: bounded channels and resource governor

- Status: Complete for the bounded, non-networked infrastructure scope
- Date: 2026-07-15
- Plan: [`022-m2-bounded-channels-resource-governor.md`](022-m2-bounded-channels-resource-governor.md)

## Scope and deviations

Plan 022 adds infrastructure only. No sockets, DNS, transport queues, peer
policy, NetDB queries, tunnel admission, destination streams, application
listeners, protocol capability claims, or live daemon startup were added.

The implementation uses concrete Tokio channels in `i2pr-runtime` and keeps
resource accounting runtime-neutral in `i2pr-core`. Limits are immutable for a
`ResourceBudget` instance and admission is immediate grant or denial; no
asynchronous resource waiter queue was added. Events use an explicit bounded
single-consumer drop-newest policy rather than implying lossless broadcast.
Latest-state channels use `watch` with initial absence, monotonic versions,
and closure detection.

The queue/resource handoff is intentionally concrete: a sender reserves a
bounded queue slot before acquiring a charge, then enqueues an item that owns
the lease through receiver handoff and processing. Dropped queued items release
their charges. The supervisor already owns service tasks and cancellation; the
new synthetic graph test demonstrates channel teardown through that existing
owner rather than adding a second task-owner abstraction.

## Changed files

- `crates/i2pr-core/src/lib.rs`: Plan 022 resource classes, bounded immutable
  budgets, high-water and denial snapshots, consuming leases, atomic bundles,
  overflow categories, and deterministic accounting tests.
- `crates/i2pr-runtime/src/channel.rs`: bounded command, request, event, and
  latest-state channel wrappers; typed send/receive outcomes; queue charges;
  privacy-safe channel snapshots; deterministic overload and cleanup tests.
- `crates/i2pr-runtime/src/lib.rs`: public runtime façade re-exports.
- `crates/i2pr-runtime/src/supervisor.rs`: preserve manager panic identity and
  stop startup when an already-started essential service fails; regression
  tests for both paths.
- `README.md`, `AGENTS.md`, `CONTRIBUTING.md`: current status, queue/deadline,
  lease-ownership, and overload-testing guidance.
- `docs/architecture.md`: channel taxonomy, capacity ceilings, resource
  ownership, bundle semantics, and snapshot fields.
- `docs/security-model.md`: queue exhaustion, slow consumer, waiter leak,
  accounting, partial-acquisition, and shutdown-starvation threats.
- `plans/022-closure.md`: this closure record.

No dependency changes were required beyond the already-approved Tokio runtime
boundary from Plan 021. `i2pr-core` remains Tokio-free and production crates
retain the existing dependency direction.

## Public contracts and limits

- Channel identifiers are nonempty ASCII-safe names capped at 64 bytes; owners
  use the existing bounded `ServiceName` type.
- Infrastructure channels have explicit capacities from 1 through 4,096.
- Caller byte estimates are capped at 1 MiB. Byte-charged sends must provide a
  nonzero estimate within the configured per-channel maximum.
- Commands and requests wait only through `send_until` with a caller deadline
  and runtime cancellation token. Immediate `try_send` reports typed full or
  closed outcomes without dropping payload ownership.
- Events are bounded single-consumer streams with drop-newest behavior and a
  counted full outcome. Latest-state values coalesce and expose version,
  current value, initial absence, cancellation, and closure.
- Request channels own one `oneshot` response path. Response closure,
  cancellation, and deadline while awaiting a response are distinct outcomes.
- Channel snapshots contain static name/owner/class/policy, capacity, queue
  depth, accepted/full/deadline/cancellation/closure/drop/resource-denial
  counters. Payloads and resource contents are not included.
- Resource budgets accept at most 32 configured classes. Plan 022 classes are
  service tasks, child tasks, command/event queue items, buffered bytes,
  simulated stream/datagram links, pending timers, and test peers; existing
  future-facing classes remain available.
- Resource limits are immutable. Usage, high-water, and denied counters are
  bounded; counter increments saturate and arithmetic overflow is a typed
  denial. Leases are non-cloneable and release exactly one grant on drop or
  consuming `release`.
- Bundle admission validates all requests, rejects zero/duplicate/missing or
  over-limit entries before mutation, orders classes deterministically, and
  commits all grants atomically.

## Test evidence

The focused Plan 022 lanes cover:

- channel name, zero/excess capacity, policy, byte-estimate, and missing-budget
  validation;
- ordered commands, immediate full rejection, cancellation-aware waiting,
  receiver-drop release, event drop accounting, latest-state versions and
  initial absence;
- request response success and response-path cleanup;
- named panic classification and essential-service startup-failure handling;
- resource exact-limit admission, denial/high-water snapshots, zero and
  `u64` overflow validation, consuming/drop/unwind release, atomic bundles,
  duplicate handling, and concurrent limit invariants;
- a synthetic producer/worker supervisor graph under capacity-one overload,
  with queue depth, resource usage, and owned-task count returning to zero.

All asynchronous tests use paused Tokio time or explicit scheduler yielding;
there are no wall-clock sleeps or network operations.

## Quality results

Final local results are recorded after the complete documentation and code
review pass:

```text
cargo fmt --all --check                         PASS
cargo check --workspace --all-targets           PASS
cargo test --workspace                          PASS (113 tests)
cargo clippy --workspace --all-targets --all-features -- -D warnings PASS
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps PASS
bash scripts/check-dependency-direction.sh      PASS
cargo deny check advisories bans sources        PASS (existing rand_core duplicate warning)
cargo +1.85.0 check --workspace --all-targets   PASS
git diff --check                                PASS
```

Known pre-existing dependency-policy warnings are called out rather than
silently suppressed. Remote CI evidence is recorded after the main-branch push
when the repository's GitHub authentication is available.

## Security and dependency decisions

The channel API does not expose raw Tokio senders, unbounded constructors, or
deadline-free service sends. Shutdown remains an out-of-band cancellation path.
Resource admission does not hold its mutex across an await. Queue payloads,
secrets, peer identities, addresses, and destinations are absent from
diagnostics. No new dependency or protocol-support status was introduced.

## Known limitations and Plan 023 handoff

- The daemon does not yet construct live services or pass channel handles at
  runtime; Plan 022 provides the contracts and synthetic evidence only.
- Broadcast events, drop-oldest policies, and priority queues remain deferred;
  later plans must select a policy explicitly rather than broaden this API by
  default.
- Resource budgets are passed explicitly to channel specifications and are not
  yet a daemon-global service locator or live configuration surface.
- Per-service supervisor grace periods remain a Plan 021 follow-up concern; the
  manager panic identity and startup essential-failure paths were corrected in
  this plan. No networking or protocol behavior was added.
- No interoperability, public-network, anonymity, privacy, or production-
  readiness evidence follows from this closure.

Plan 023 may build deterministic in-memory stream/datagram links on these
bounded ownership and cancellation contracts. It must keep link queues and
fault workloads bounded and must not promote simulation into live transport
support.
