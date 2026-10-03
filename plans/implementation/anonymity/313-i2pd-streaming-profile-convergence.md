# Plan 313 — i2pd-compatible Streaming profile convergence

Status at registration: **blocked-on-plan312**

Classification: capability + corrective convergence.

Hard dependency: Plan 312 passed with executed two-family directional evidence.

## 1. Objective

Tune only the remotely observable Streaming dimensions proven by Plan 312 to differ between i2pr and pinned i2pd 2.61.0, producing one coherent i2pd-compatible service Streaming profile.

## 2. Why ready only after Plan 312

The prior Plan 299 had source-level candidate differences but no trustworthy observation matrix. Plan 312 replaces the unattainable three-family hostile-reference gate with executed, directional i2pd evidence.

## 3. Invariants

1. No source-constant-only tuning.
2. No Java compatibility gate.
3. No randomization used to hide differences.
4. Correctness, congestion safety, retransmission ceilings, cancellation, and resource bounds may not be weakened merely to copy i2pd.
5. Unobservable dimensions remain unchanged.
6. One coherent target profile is used; do not cherry-pick incompatible behaviors.

## 4. Scope

Centralize the qualified service Streaming profile; modify measured handshake/ACK/window/RTO/retransmission/close behavior as justified; preserve generic local Streaming APIs; rerun the exact Plan 312 directional evidence.

## 5. Required production changes

1. Freeze the target matrix from Plan 312 before code changes.
2. Map each observed dimension to the active i2pr state transition/config value.
3. Centralize service-profile values separately from hard safety ceilings.
4. Make the minimum changes required to match the measured i2pd behavior.
5. Re-run deterministic correctness/loss/reorder/close tests.
6. Re-run the exact Plan 312 directional evidence.
7. Classify residual differences as matched, not reliably observable, or intentional safety/protocol divergence.

## 6. Ordered work packages

WP1 target freeze; WP2 active-value/state mapping; WP3 narrow production edits; WP4 deterministic correctness/safety; WP5 exact directional rerun; WP6 residual disposition.

## 7. Failure, cancellation, restart, and contention

Existing Streaming manager semantics remain bounded. Any target behavior that creates unbounded retransmission, queue growth, or cancellation ambiguity is rejected as a safety divergence and documented.

## 8. Compatibility and migration

Expose one named i2pd-compatible service profile internally/default for service Destination groups after qualification. Avoid a combinatorial public knob surface. Existing explicit expert/raw behavior may remain where already supported.

## 9. Required tests

Every changed dimension gets pre/post deterministic tests; congestion/retransmission safety; close/reset; loss/reorder; resource ceilings; service interop regressions; exact Plan 312 rerun.

## 10. Exact verification commands

Full workspace/clippy/docs floor, focused `i2pr-client` Streaming suite, service-tunnel integration suite, Plan 312 evidence checker/runners, and boundary scripts.

## 11. Documentation updates

State exactly which observable dimensions match pinned i2pd, which remain safety/implementation divergences, and that this is not general anonymity or Java equivalence.

## 12. Acceptance criteria

No unresolved high-confidence i2pr-only difference remains in the qualified measured dimensions unless explicitly retained for stronger safety/correctness; all retained differences have evidence and rationale; full floor is green.

## 13. Stop conditions

Stop on target behavior that conflicts with protocol correctness/resource safety or cannot be reproduced reliably in Plan 312.

## 14. Closure evidence required

Target matrix, production diff, deterministic tests, repeated directional captures, residual classification, and exact command results.

## 15. Handoff

A pass closes the Streaming branch required by the future integrated anonymity successor.
