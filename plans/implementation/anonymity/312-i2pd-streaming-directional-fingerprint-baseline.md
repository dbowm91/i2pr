# Plan 312 — i2pd Streaming directional fingerprint baseline

Status at registration: **blocked-on-plan310**

Current dependency authority: **blocked-on-plan315**. Plan 310 remains an immutable blocked record; Plans 314–315 are its registered corrective sequence, and Plan 315 must pass before this evidence lane executes.

Classification: evidence infrastructure only.

Hard dependencies: Plan 315 passed (which transitively requires Plan 314); ADR 0030; exact i2pd 2.61.0 pin.

## 1. Objective

Measure externally observable Streaming behavior for pinned i2pd and i2pr in both client and server roles using a controlled opposite endpoint owned by i2pr, producing a sanitized directional differential baseline without requiring a test seam inside i2pd.

No production Streaming tuning is allowed in this plan.

## 2. Measurement model

Client-direction lane: drive i2pd client traffic through an ordinary proxy/client tunnel to an i2pr-controlled raw Streaming observation Destination. Compare with i2pr client behavior under the same stimulus.

Server-direction lane: drive a controlled i2pr raw/probe client against an i2pd server tunnel and an i2pr server group under the same stimulus.

The controlled i2pr side may suppress its normal automatic Streaming responses only in test-only probe code so it can observe packet timing/options. i2pd remains unmodified.

## 3. Why this is sufficient

Plan 304 stopped because it required stock Java and i2pd to expose an internal hostile-Destination packet-response seam. ADR 0030 selects a single coherent Streaming target instead. External behavior can be measured with i2pr controlling the opposite endpoint; the reference router does not need to expose an invasive API.

## 4. Candidate observable dimensions

Handshake flags/options; FROM inclusion; maximum packet size; initial send window; advertised receive window; ACK delay; NACK behavior; initial RTO; retransmission schedule/count; choke/unchoke; duplicate/reorder response; close/reset behavior; packet sizing; terminal state.

Exact source review freezes candidate dimensions before execution, but only observed dimensions may justify Plan 313 tuning.

## 5. Invariants

Exact i2pd source/binary pin; no reference patching; private/loopback topology; no public reseed; identical scripted stimuli; sanitized metadata only; bounded events/deadlines/repetitions; no Java pass/fail dependency; probe-only seams inaccessible from ordinary production composition.

## 6. Required implementation

Reuse/extend `i2pr-testkit::streaming_fingerprint` for two-family directional traces; add raw observation/probe seams on the i2pr test side; add i2pd client and server runners; add deterministic scenario subset expressible from the controlled side; add classifier and evidence checker.

## 7. Failure, cancellation, restart, and contention

Hard startup/scenario deadlines; fresh reference state for counted runs; missing observation is `NotReliablyObservable`, not a guessed value; cleanup owns all child PIDs; no retries after a frozen attempt budget.

## 8. Compatibility and migration

Test-only infrastructure. No production config or Streaming constants change.

## 9. Required tests

Trace canonicalization/redaction; directional classification; timing buckets; missing-observation handling; cleanup; pin mismatch; scenario bounds; probe isolation from production composition.

## 10. Exact verification commands

Ubuntu preflight, exact i2pd artifact verification, focused testkit/client suites, directional runner commands, evidence checker, and ordinary workspace floor.

## 11. Documentation updates

Record source candidate matrix and executed two-family directional differential. Java behavior may be noted separately but cannot fail this plan.

## 12. Acceptance criteria

At least one valid client-role and one valid server-role baseline exists for i2pd and i2pr across every registered reliably observable dimension; evidence is sanitized and reproducible; no production tuning landed.

## 13. Stop conditions

Stop if a dimension cannot be observed without modifying i2pd, if the controlled i2pr probe itself necessarily changes the dimension being measured, or if topology cannot be isolated.

## 14. Closure evidence required

Exact pin/hash, scenario manifest, traces/distributions, classifier matrix, evidence checker results, and full test floor.

## 15. Handoff

On pass, Plan 313 becomes dependency-ready.
