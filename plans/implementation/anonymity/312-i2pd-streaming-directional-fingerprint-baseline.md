# Plan 312 — i2pd Streaming directional fingerprint baseline

Status: **passed-pinned-i2pd-directional-handshake-fingerprint-baseline**

Dependency authority at execution: **ready-after-plan315**. Plan 310 remains an immutable blocked record; Plans 314–315 are its registered corrective sequence, and Plan 315 passed.

Classification: evidence infrastructure only.

Hard dependencies: Plan 315 passed (which transitively requires Plan 314); ADR 0030; exact i2pd 2.61.0 pin.

## 1. Objective

Measure externally observable Streaming behavior for pinned i2pd and i2pr in both client and server roles using a controlled opposite endpoint owned by i2pr, producing a sanitized directional differential baseline without requiring a test seam inside i2pd.

No production Streaming tuning is allowed in this plan.

## 2. Frozen measurement model and observed dimensions

Client-direction lane: drive i2pd client traffic through an ordinary proxy/client tunnel to an i2pr-controlled raw Streaming observation Destination. Compare with i2pr client behavior under the same stimulus.

Server-direction lane: drive a controlled i2pr raw/probe client against an i2pd server tunnel and an i2pr server group under the same stimulus.

The executed lane used the Plan 193 exact-pinned i2pd black-box setup. The ignored test driver owns the i2pr endpoint and captures the initial handshake packet at each endpoint role. i2pd remained unmodified. The bounded scenario is `clean_handshake_default_port`; one SYN or SYN-ACK is retained for each of `i2pr-client`, `i2pd-client`, `i2pr-server`, and `i2pd-server`.

## 3. Why this is sufficient

Plan 304 stopped because it required stock Java and i2pd to expose an internal hostile-Destination packet-response seam. ADR 0030 selects a single coherent Streaming target instead. External behavior can be measured with i2pr controlling the opposite endpoint; the reference router does not need to expose an invasive API.

## 3. Candidate dimension disposition

Only these source-observable handshake dimensions are registered for this run: flags, FROM inclusion, maximum packet payload, and initial payload length. The captured payload length is zero in all four roles. Flags and FROM inclusion match between implementations; maximum packet payload differs (i2pr 1730, i2pd 1812) in both client and server roles.

Window/choke, ACK/NACK, RTO/retransmission, loss/reorder, close/reset, and terminal-state dimensions were not observed by this handshake-only capture and are `NotReliablyObservable` in this registered scenario. They are not evidence for or against Plan 313 tuning. A future expansion requires a new bounded scenario and a recorded source-observability review before execution.

## 4. Invariants

Exact i2pd source/binary pin; no reference patching; private/loopback topology; no public reseed; identical scripted stimuli; sanitized metadata only; bounded events/deadlines/repetitions; no Java pass/fail dependency; probe-only seams inaccessible from ordinary production composition.

## 5. Required implementation and delivered surface

The ignored daemon integration target reuses the canonical `i2pr-testkit::streaming_fingerprint` schema by including its source directly, avoiding a forbidden daemon-to-testkit package dependency. It captures only decoded handshake metadata, discards raw packet bytes and identities, and writes one trace per implementation/role. The Plan 193 lane supplies the exact-pinned i2pd client and server interactions. The Plan 312 runner applies the evidence checker and writes a sanitized artifact manifest and comparison matrix.

## 6. Failure, cancellation, restart, and contention

Hard startup/scenario deadlines; fresh reference state for counted runs; missing observation is `NotReliablyObservable`, not a guessed value; cleanup owns all child PIDs; no retries after a frozen attempt budget.

## 7. Compatibility and migration

Test-only infrastructure. No production config or Streaming constants change.

## 8. Required tests

Evidence checker tests cover a valid four-role matrix, a missing role, a mismatched i2pd pin, and an extra trace column. The external lane verifies exact pinning, cleanup, both client/server roles, and both directions. Timing buckets are not registered because this scenario captures no timing behavior.

## 9. Exact verification commands

`tests/integration/anonymity/run-plan312-streaming.sh` ran to completion with the exact pin and produced the sanitized matrix. Supporting verification: `cargo fmt --all`; `cargo check --locked -p i2pr-daemon --test streaming_tunnel_external`; `bash scripts/check-dependency-direction.sh`; `bash scripts/check-runtime-boundaries.sh`; and `python3 -m unittest discover -s tests/integration/anonymity -p 'test_streaming_fingerprint.py'`.

## 10. Documentation updates

The exact pin, scenario, registered dimensions, unobserved candidate dimensions, and differential result are recorded in `plans/closure/anonymity/312-status.md`. Java behavior is outside this plan and does not affect its result.

## 11. Acceptance criteria

One valid client-role and one valid server-role baseline exists for i2pd and i2pr across all four registered dimensions; the exact-pinned run and sanitized evidence checker pass; no production tuning landed.

## 12. Stop conditions

Stop if a dimension cannot be observed without modifying i2pd, if the controlled i2pr probe itself necessarily changes the dimension being measured, or if topology cannot be isolated.

## 13. Closure evidence

Exact pin/hash, scenario manifest, four metadata traces, classifier matrix, and evidence checker result are recorded under `target/interop/anonymity/plan312-streaming` for this run. Raw packet bytes, Destination identifiers, and stream identifiers are not retained. The evidence directory is generated and ignored; hashes are recorded in the sanitized `evidence.json`.

## 14. Handoff and unblock audit

Plan 313 is dependency-ready and moves to `ready`. Plan 311 remains blocked pending Plan 316 because it has an independent normal-daemon lifecycle integration gap. Plan 308 remains blocked on its separate controlled ordinary-HTTP topology and three-family captures. No other registered plan is unblocked by this fingerprint result.
