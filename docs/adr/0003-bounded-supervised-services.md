# ADR 0003: Bounded supervised services and explicit cancellation

- Status: Accepted
- Date: 2026-07-15

## Context

Router peers and clients are untrusted, and long-lived asynchronous work can
otherwise leak tasks, queues, buffers, and resource leases during failure.

## Decision

Every long-lived service will be owned by a supervisor and classified as
essential, restartable, degradable, or optional. Queues and buffers are
bounded. Work has explicit deadlines, cancellation, and cleanup. Resource
leases release on success, failure, timeout, and cancellation; detached
long-lived tasks are not permitted.

## Consequences

Failure behavior is visible in lifecycle and health snapshots, and resource
exhaustion has an explicit policy. Implementations require more bookkeeping and
negative tests, but memory growth and orphaned work do not become implicit
control flow.

## Alternatives

Unbounded queues were rejected because they turn peer-controlled input into
memory pressure. Detached tasks were rejected because ownership and shutdown
cannot be audited reliably. A single global executor policy was rejected in
favor of service-owned cancellation and resource capabilities.

## Review triggers

Review when the first real service graph is implemented, when queue semantics
are selected, or when a resource class needs a policy not expressible by the
shared governor contracts.
