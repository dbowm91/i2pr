# ADR 0002: Tokio at runtime-facing boundaries

- Status: Accepted
- Date: 2026-07-15

## Context

The eventual router has long-lived network services and supervised tasks, while
protocol parsing and state-machine logic benefit from deterministic, runtime-
neutral tests.

## Decision

Tokio is the expected runtime at daemon and runtime-service boundaries when a
detailed plan requires asynchronous work. Protocol vocabulary, pure validation,
and state-machine contracts should remain runtime-neutral where practical.
The bootstrap CLI is synchronous because it opens no listeners and does not
benefit from introducing a runtime dependency yet.

## Consequences

Future async services can use standard Tokio cancellation and bounded channels
at the composition boundary without forcing timers or executors into protocol
crates. The testkit can control protocol time without abstracting every Tokio
timer API.

## Alternatives

Using Tokio throughout all crates was rejected as premature coupling. Avoiding
an async runtime entirely was rejected because the planned router requires
supervised concurrent I/O.

## Review triggers

Revisit if a later implementation plan selects another maintained runtime or
demonstrates that the service model needs a different ownership mechanism.
