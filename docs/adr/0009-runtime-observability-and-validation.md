# ADR 0009: Privacy-aware runtime observability and deterministic validation

- Status: Accepted
- Date: 2026-07-15

## Context

Plans 021–023 provide supervision, bounded communication/resource ownership,
and a deterministic testkit. Operators and tests still need a common way to
inspect lifecycle and cleanup without turning diagnostics into a second data
retention path or leaking peer-controlled values.

## Decision

Use fixed event names under the `i2pr.runtime` and `simulation` targets. Events
may contain only validated static identifiers, typed categories, bounded
counters with units, monotonic timing, and synthetic link/sequence/rule
metadata. Lower crates emit events but never install subscribers; the daemon
owns subscriber configuration.

Expose bounded aggregate snapshots assembled from a redacted supervisor
projection, channel snapshots, resource usage, and optional simulation
counters. Entries are sorted and capped. Snapshot generation is synchronous,
does not await while holding mutable state, and is eventually coherent. Health
detail is not included in the aggregate projection, and its default `Debug`
representation is redacted.

Validate the complete stack with paused Tokio time/manual-clock scenarios for
clean shutdown, overload, restart recovery, essential failure/forced abort,
and stream/datagram faults. Every case has a stable scenario identifier, fixed
root seed where simulation is involved, and a bounded step/test budget.

## Consequences

Runtime state is diagnosable without retaining raw protocol data, and failures
can be replayed locally. Aggregate snapshots are not transactional across
independent channel/resource/simulation owners, so callers must treat them as
eventually coherent. Detailed operator troubleshooting needs typed categories
or a later reviewed diagnostic surface rather than arbitrary error strings.

Snapshots also depend on confirmed ownership evidence: a zero service or child
task count is published only after the corresponding manager or child join has
completed. Forced manager cleanup uses the supervisor-retained child scope;
remaining child handles and cleanup-invariant failures remain visible in the
typed shutdown report rather than being hidden by counter decrements.

The five integrated scenarios and 32-seed matrix add deterministic test time
but no network features, protocol support evidence, persistent event history,
metrics exporter, or administrative listener.

## Alternatives considered

- Raw `Debug` or error payloads were rejected because they can contain secrets,
  peer input, and filesystem paths.
- An unbounded event log, Prometheus endpoint, or persistent diagnostics store
  was deferred because it creates retention and high-cardinality policy that
  is not needed before transport planning.
- OS-random exploratory tests were deferred from the required matrix because
  reproducibility and failure retention are more important at this boundary.

## Review triggers

Review this decision before adding transport peer labels, an operator-facing
listener/exporter, persistent event retention, or identity-rich debug modes.
