# Plan 445 — Current-pin NTCP2 control runner attribution corrective

Status: **registered / ready**. Corrective successor to Plan 441, which stopped
after its first stock-to-stock attempt returned exit 66 without preserving a
bounded terminal category. Plan 441's historical stopped result remains
immutable. Hard dependencies: Plan 440 passed; pinned helper source and build
are available at i2pd `2.61.0` / `635b013a612ff47278ef02acf8580a28e10e26c5`.
Roadmaps: core-router recovery and NTCP2 transport.

## Objective and classification

Correct the source-evidenced evidence defect in Plan 441's stock-control runner,
then execute one fresh bounded Plan 441 acceptance pass. The fresh pass must
prove stock-to-stock topology before any i2pr protocol attribution and must
retain a useful sanitized failure category if a gate still fails.

**Infrastructure:** failure-stage attribution, fixed receipt schema, fresh
stock positive-control budget, and renewed one-session i2pr forward/reverse
evidence if the positive control passes. **Invariant:** no additional attempt
after a new unclassified live failure; no public NTCP2 activation, raw logs,
router hashes, endpoints, secrets, or packet bytes in persistent evidence.
**Capability:** none; this remains a controlled one-family evidence lane.

## Current evidence and defect

Plan 441's current-pin helper build completed from the pristine pinned source.
The first stock-to-stock attempt launched separate loopback helper processes
with fresh keys, but both exited 66. The receipt retained `environment-blocked`
and stage counts only. `run_plan441_stock_control.py` did not project the
helper's bounded `terminal_rejected.reason_code`, so the attempt cannot be
localized and is not NTCP2 protocol-rejection evidence.

## Scope and ordered work

1. Extract only the first terminal rejection's bounded category from helper
   events; reject arbitrary strings and never persist process output or event
   identity fields. Include runner digest, fixed result vocabulary, and a
   self-test proving useful categories survive while path-like/dynamic detail
   is rejected.
2. Reproduce all configs with separate loopback endpoints, persistent distinct
   identities, network ID 2, no external reseed, and stock i2pd source untouched.
3. Allocate a fresh budget: one stock-to-stock pair. Require both processes'
   authenticated state, decoded type-10 I2NP, actual reply send/receive, and
   clean shutdown. On any failure, retain the first sanitized reason and stop.
4. If and only if the stock control passes, run one forward and one reverse
   i2pr↔stock pair with per-invocation correlation and the exact Plan 441
   acceptance oracle. Do not alter wire code without a source-localized defect.
5. Record resource cleanup and final role outcomes. Preserve Plan 441's blocked
   status and mark it `superseded-by-plan445` only if this corrective satisfies
   every remaining requirement; otherwise leave the line blocked with the
   exact new evidence boundary.

## Failure, cancellation, compatibility, and security

Every process has a strict deadline and one owner. Timeout handling terminates
and then kills only the owned helper processes; disposable identity roots are
removed on every exit path. Receipt fields are allowlisted enums, counters,
and return categories. No retries occur within a failed control or direction.
No production config, RouterInfo, static-key rotation, dependency, or support
claim changes are in scope.

## Verification

- `python3 tools/i2pr-interop/reference/i2pd-current/run_plan441_stock_control.py --self-test`
- `python3 scripts/check-current-pin-ntcp2-runner.py --self-test`
- `cargo test --locked -p i2pr-transport-ntcp2 --all-targets`
- `cargo test --locked -p i2pr-runtime --all-targets`
- `cargo test --locked -p i2pr-interop --all-targets`
- `bash scripts/check-ntcp2-vectors.sh`
- One fresh stock-to-stock run; i2pr forward/reverse only after its positive control passes.
- `cargo fmt --all --check`, `cargo check --locked --workspace --all-targets`, and the full routine floor.

## Acceptance and stop conditions

Pass the corrective only when the fresh stock control passes, its receipt is
sanitized and session-bound, and the full Plan 441 forward/reverse acceptance
passes. A failed positive control, missing correlation, or unclassified failure
stops immediately and remains a fixture/environment blocker. No supported
public NTCP2 claim is permitted. Closure: `plans/closure/ntcp2-transport/445-status.md`.
