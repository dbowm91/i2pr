# Plan 424 — attribute reverse responder SessionConfirmed I/O operation

Status: **active** — `in-progress-reverse-responder-io-operation-attribution`.
Corrective successor to [Plan 423](../../closure/ntcp2-transport/423-status-amendment-plan-424.md)
and [Plan 434](../../closure/ntcp2-transport/434-status-amendment-plan-424.md).

## Objective and readiness

Preserve the exact responder I/O operation and bounded `IoErrorKind` when the
current-pin reverse attempt fails during `await_confirmed`, emit one fixed
status code, and spend one reverse-only attempt to learn whether the responder
failed while writing SessionCreated or reading SessionConfirmed.

Plan 423's evidence records the responder's `io_failed` category and the
helper's `session-not-established` outcome, but neither carries a shared
session token. Source tracing found that `ResponderStage` records only the
state-machine phase. In `await_confirmed`, the runtime executes both
`Write(SessionCreated)` and `ReadExact(SessionConfirmed)`; either I/O failure
currently maps to the same status. The existing bounded action counters
distinguish those operations, but are discarded by the current wrapper.

## Classification and invariants

**Infrastructure/diagnostic only.** This plan adds exact error attribution; it
does not implement or claim an NTCP2 capability.

1. Preserve protocol behavior, wire bytes, deadlines, cancellation, listener
ownership, and authenticated-link promotion. No protocol algorithm changes.
2. Error attribution uses a closed enum for the I/O operation and existing
   `IoErrorKind`; never serialize dynamic error text, peer bytes, identities,
   endpoints, keys, or payloads.
3. The runtime's `HandshakeDriverError` remains typed and bounded. Normal
   production behavior is unchanged; diagnostics are consumed by the
   non-production interop launcher.
4. Preserve the pinned i2pd revision
   `635b013a612ff47278ef02acf8580a28e10e26c5`, network ID 2, and loopback-only
   topology. Do not modify or instrument the reference implementation.
5. Spend exactly one reverse-only attempt. No forward rerun, retry, or use of
   the historical Plan 099/100 lane.
6. Normal-daemon NTCP2 remains disabled and non-advertised. Plan 434 still
   requires authenticated bidirectional NTCP2 and correlated I2NP delivery.
7. If the operation attribution remains insufficient to localize a safe
   correction, stop; this plan does not authorize another diagnostic loop.

## Scope and ordered work

1. Add a bounded responder I/O operation to the runtime error context,
   derived from the already-maintained completed read/write counters. For the
   `await_confirmed` phase, distinguish the first pending write
   (`SessionCreated`) from the following read (`SessionConfirmed`). Preserve
   the existing `ExactIoError.kind`.
2. In `tools/i2pr-interop`, map the fixed operation/kind combinations to
   distinct closed terminal `StatusReason` values. Keep non-I/O protocol,
   part-two, and RouterInfo identity classifications intact.
3. Extend the evidence projection with only the new exact status codes. Add
   exhaustive no-process mapping tests and checker mutations that reject
   collapsed operation mapping, lost I/O kind, and unlisted reason codes.
4. Run focused tests, the full production workspace floor (the runtime error
   contract changes), and the pinned helper build. Then execute exactly one
   reverse-only attempt.
5. Record whether the error occurred on SessionCreated write or SessionConfirmed
   read, the bounded I/O kind, helper result, cleanup, and correlation limits.
   Do not claim a common cause without an explicit correlation token.

## Failure and acceptance

The source checker and exhaustive no-process tests must pass, and all required
workspace checks must pass. The single attempt must produce one allowlisted
operation/kind status or authenticate the session and deliver correlated
I2NP. A still ambiguous result closes this plan as stopped with the exact
remaining evidence gap; no further wire attempt or protocol fix is implied.

Plan 434 passes only on authenticated forward and reverse NTCP2 plus
correlated I2NP delivery. This diagnostic alone cannot unblock Plans 433,
435–439, or Plan 412.

## Handoff and closure evidence

Record implementation commits, requirement matrix, exact commands and results,
full-floor outcome, pristine helper pin/build, one-attempt sanitized evidence,
teardown, security/privacy review, Plan 434 disposition, and a fresh unblock
audit. No reference logs or raw protocol data may be retained in the closure.
