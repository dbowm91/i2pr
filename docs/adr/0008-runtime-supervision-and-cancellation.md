# ADR 0008: Concrete runtime ownership and wakeable supervision

- Status: Accepted
- Date: 2026-07-15

## Context

Plan 021 is the first milestone that owns long-lived asynchronous work. The
router will eventually have transport, NetDB, tunnel, client, API, and local
service tasks, but this milestone must remain non-networked and protocol-
agnostic. A runtime-neutral contract in `i2pr-core` is useful to those later
subsystems, while task ownership, timers, wakeups, and joins require a concrete
executor boundary.

## Decision

Create `i2pr-runtime` between `i2pr-core` and `i2pr-daemon`:

```text
i2pr-proto <- i2pr-crypto <- i2pr-storage
      ^
  i2pr-core <- i2pr-runtime <- i2pr-daemon
```

`i2pr-core` remains free of Tokio and owns only semantic identifiers,
classifications, lifecycle transitions, health vocabulary, typed failure
categories, cancellation reasons, and synchronous resource contracts.
`i2pr-runtime` owns the concrete service graph, readiness, latest-state health
watchers, service managers, child scopes, restart policy, timers, and
graceful/forced shutdown. The daemon remains the composition root, while its
live CLI command stays disabled until a later plan wires a real service graph.

Tokio is selected as the initial concrete runtime. The workspace requirement is
Tokio `1.48` (the closure lockfile resolves `1.52.3`), and enables only
`macros`, `rt`, `sync`, `time`, and `test-util`; no TCP, UDP, DNS, or other
network feature is enabled. `test-util` supports paused deterministic tests
and explicit `time::advance`, not production networking. Rust 1.85 remains the
declared MSRV and is checked separately.

`tokio-util::sync::CancellationToken` is selected through the crate's narrow
`rt` feature. The runtime wraps it with a bounded `CancellationReason` store to
provide first-reason-wins semantics and privacy-safe hierarchical reporting.
Tokio's cancellation primitive provides wakeable cancellation, parent-to-child
propagation, and race-safe registration; the runtime-neutral atomic token is
not used as an asynchronous wait primitive.

`futures-util` is enabled only with `std` for `FutureExt::catch_unwind`, so a
service panic becomes a static `Panic` completion and never exposes a panic
payload. All managers live in an owned Tokio `JoinSet`; a service child scope
owns its own bounded `JoinSet`. No task handle is discarded.

Startup validates all registrations before spawning, then starts services in a
deterministic dependency-first order. Readiness is an explicit one-shot
signal. Only `Restartable` services may use an explicit bounded backoff policy;
essential, degradable, and optional services do not restart implicitly.
Shutdown cancels every scope, joins within a bounded deadline, aborts remaining
managers, and joins aborted handles before returning a report.

## Alternatives considered

- Reusable supervision inside `i2pr-daemon` was rejected because future
  transport, NetDB, tunnel, and client crates would then need to depend on the
  composition root. The narrow runtime crate localizes replacement and keeps
  the daemon as the process root.
- A broad runtime trait or universal async service abstraction was rejected.
  It would spread executor portability claims through protocol and state
  contracts without a current second runtime to validate those claims.
- Tokio throughout every crate was rejected as premature coupling and would
  make deterministic protocol code depend on an executor.
- Detached subsystem-owned spawning was rejected because task ownership,
  cancellation, panic observation, and shutdown cleanup could not be audited.
- A polling-only cancellation flag was rejected at async boundaries because it
  cannot wake blocked work or prove that wait registration has no lost-wakeup
  race. It remains available only for synchronous core contracts.
- A custom cancellation primitive was rejected for this milestone because the
  selected Tokio utility already provides the required hierarchical wakeup
  behavior with a smaller correctness surface; the reason store remains local
  and bounded.

## Dependency and security posture

Tokio and `tokio-util` are runtime-facing dependencies only. No network feature
is enabled and no Plan 021 code opens a socket, performs DNS, touches NetDB,
constructs tunnels, or advertises protocol capabilities. Dependency versions
are centralized in the workspace manifest and checked by the dependency
direction script, `cargo deny`, normal workspace checks, and the Rust 1.85
MSRV lane.

Service names, health details, service counts, timeouts, child tasks, restart
attempts, and restart delays are bounded. Health and completion values retain
static categories rather than raw errors. Panic payloads and arbitrary user
text are redacted. Forced abort is reported as cleanup evidence and does not
claim that non-cooperative code performed graceful cleanup. The workspace
continues to use `panic = "unwind"` so manager tasks can classify panics.

## Plan 025 corrective amendment

The accepted ownership model includes one bounded child-scope owner slot per
active service manager. Normal manager execution retains the child `JoinSet`
inside its `ChildScope` and joins every result before returning. On a forced
manager abort, the supervisor retains that same scope, aborts its children,
and drains the collection before final reporting. A scope's synchronous
`Drop` path may request abort, but it never decrements confirmed-task counters
or claims cleanup completion. A bounded drain that cannot confirm termination
is surfaced as `FailedCleanup` with remaining-child evidence.

Service completion is classified from observed cancellation state. A
`RequestedShutdown` result without service, manager, or root cancellation is
an unexpected clean exit and follows the service classification policy; only a
cancellation-driven result is a clean requested shutdown. This prevents a
service-selected enum value from silently removing an essential service.

## Consequences

The runtime boundary is concrete rather than portable, but later replacement
is localized to `i2pr-runtime` and its daemon composition adapter. Supervised
services require more registration and cleanup bookkeeping, and deterministic
tests must use paused time rather than scheduler assumptions. Plans 022–024
must extend the bounded ownership model without adding detached tasks or
runtime dependencies to protocol crates.

## Review triggers

Review this decision if another maintained runtime is selected, if a later
plan requires network features, if task ownership must cross a crate boundary,
or if cancellation reasons need a new privacy-reviewed category.
