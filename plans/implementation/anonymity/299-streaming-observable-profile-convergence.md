# Plan 299 — Streaming observable-profile convergence

Status: stopped-no-plan-298-differential-evidence

Classification: corrective capability.

Hard dependencies: Plan 298 closed with executed three-family differential evidence.

## Objective

Adjust i2pr's remotely observable Streaming behavior so the qualified profile converges on one coherent deployed I2P compatibility target, using Plan 298 black-box evidence rather than isolated source constants.

## Why ready after Plan 298

Plan 298 identifies which differences are observable/stable enough to classify. Only those retained findings justify production tuning.

## Current implementation evidence

Likely candidates include initial congestion-window and initial-RTO differences, while maximum payload appears convergent. Delayed ACK, max-resend, window growth, close behavior, and loss response remain evidence-dependent. Active values must be traced from `StreamingConfig::balanced()`, not confused with hard ceilings.

## Invariants

- Preserve Streaming correctness and M6/i2pd interop.
- Select one coherent reference/common profile; do not build a novel per-field chimera without evidence.
- No random jitter/per-connection randomization solely for evasion.
- Do not weaken hard safety/resource ceilings just to match.
- Changes stay runtime-neutral inside i2pr-client.
- Every changed knob has focused tests and Plan 298 before/after observation.

## Scope

### In

Only Plan 298 dimensions classified high-confidence, remotely observable, and i2pr-distinguishing: handshake options/advertised size if needed, initial send window, delayed ACK, RTO behavior, retransmission limit/schedule, window growth/choke, close/reset shape.

### Out

Noisy/unobservable dimensions, tunnel path/timing, HTTP/application behavior, unrelated throughput tuning, protocol extensions.

## Required production changes

### A. Select target profile first

Convert Plan 298 matrix into target decision table. Preference:
1. behavior common to both pinned families;
2. current I2P documented default when both families are semantically compatible;
3. one coherent pinned family when unavoidable.

If no coherent target is possible without substantial interop regression, stop for ADR/corrective rather than guess.

### B. Centralize qualified values

Make active qualified Streaming profile explicit in one constructor/module with evidence comments. Separate active defaults from hard safety ceilings. Avoid duplicated externally relevant literals.

### C. Implement measured corrections only

Change each retained distinguishing dimension with focused tests. Do not change already-convergent dimensions cosmetically.

### D. Re-run hostile-Destination differential

Execute exact Plan 298 matrix before/after. Required dimensions must enter target equivalence class; unrelated dimensions must not regress.

### E. Performance/safety guard

Run enough throughput/resource correctness to detect accidental queue growth, retransmission storms, deadlocks, or pathological latency. This is regression protection, not benchmark ranking.

## Ordered work packages

1. Freeze target table from Plan 298 closure.
2. Trace active config mapping and separate values/ceilings in assertions.
3. Implement measured corrections.
4. Run local correctness/resource suite.
5. Re-run full differential matrix.
6. Record residual intentional differences.
7. Run routine floor/docs.

## Failure/cancellation/restart/contention

No new task model. Retransmission timers remain bounded/cancellable and release resources on close/reset/cancel. No retry loop becomes unbounded.

## Compatibility and migration

No on-disk migration. Public overrides remain where supported, but docs distinguish custom values from qualified default and note fingerprint risk.

## Required tests

- qualified defaults vs hard ceilings asserted separately;
- Plan 298 scenarios before/after;
- loss/reorder/retransmit correctness;
- send/receive window bounds;
- close/reset/cancel;
- bounded custom override;
- existing mixed-router/i2pd Streaming acceptance.

## Exact verification commands

~~~text
cargo fmt --all --check
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
bash tests/integration/anonymity/run-streaming-fingerprint-i2pr.sh
bash tests/integration/anonymity/run-streaming-fingerprint-i2pd.sh
bash tests/integration/anonymity/run-streaming-fingerprint-java.sh
bash scripts/check-streaming-fingerprint-evidence.sh
bash tests/integration/m6-interop/run-streaming.sh
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
~~~

Use exact existing M6 runner at execution and record it in closure.

## Documentation updates

Document qualified Streaming profile and custom-override caveat. Do not claim indistinguishability beyond scenario matrix.

## Acceptance criteria

Plan 299 passes only if:
1. every production change maps to executed Plan 298 finding;
2. coherent target recorded before tuning;
3. required observable dimensions enter target equivalence class on repeated black-box runs;
4. Streaming correctness/interop/resource gates remain green;
5. active values not confused with ceilings;
6. residual differences explicit;
7. no randomization.

## Stop conditions

Stop if convergence causes unresolved correctness/interop regression, Plan 298 evidence cannot classify proposed change, or matching requires weakening hard safety/resource ceiling without protocol justification.

## Closure evidence required

Retain target table, code/evidence mapping, before/after summaries, correctness/resource/mixed-router results, routine floor, and residual differences.

## Handoff notes

Plan 301 consumes this closure. Future performance tuning that changes observable defaults must re-run the fingerprint lane.
