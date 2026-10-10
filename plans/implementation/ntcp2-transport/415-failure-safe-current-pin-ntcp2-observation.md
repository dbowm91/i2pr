# Plan 415 — preserve sanitized evidence for current-pin NTCP2 attempts

Status: **ready** — `registered-failure-safe-current-pin-ntcp2-observation`.
Corrective successor to Plan 414; see `plans/closure/ntcp2-transport/414-status.md`.

## Objective

Repair the Plan 414 runner so every terminal path writes an atomic sanitized
attempt record outside its disposable state directory before cleanup. Negative-test
failure capture without starting reference processes. After that gate passes, run
at most one fresh current-pin loopback attempt per direction to determine whether
the stock-log observation works in practice.

## Why ready

- Plan 410 provides the explicit network-ID-2 loopback profile and pristine
  pinned i2pd helper.
- Plan 414 source-verified the post-decrypt, post-frame-parse type-10 log path,
  added the bounded observer, and recorded the runner's failure-evidence loss.
- The failure is localized to the Plan 414 orchestration/cleanup path. No
  protocol divergence or unresolved architectural decision was observed.
- Normal-daemon NTCP2 remains disabled and non-advertised under Plan 101.

## Invariants

1. Use only stock i2pd 2.61.0, revision
   `635b013a612ff47278ef02acf8580a28e10e26c5`; never modify reference source
   or enable observer instrumentation.
2. Keep all endpoints on `127.0.0.1`, network ID 2 only in the
   `current-network-loopback` topology, and reseed/public egress disabled.
3. Freeze `MAX_ATTEMPTS=1` per direction before live execution. Do not retry
   after a failed attempt. Stop the sequence on its first failed direction.
4. On every terminal path, atomically write only fixed-category outcomes,
   bounded counts, return codes, pin, direction, and permitted digests outside
   the work tree. Never retain Router Hashes, message IDs, event contents, raw
   logs, identity state, or process stdout/stderr.
5. Capture digests and sanitized stage projections before deleting the attempt
   tree. Cleanup failure is a typed non-pass and cannot erase the sanitized
   failure record.
6. Plan 415 proves only the bounded one-message observation seam. It does not
   close Plan 434's additional fragmented-message, malformed-input, teardown,
   or full discrepancy-recovery requirements.

## Scope

In scope: hardening `run_plan414.py` failure reporting and process cleanup;
adapter use on both success and failure; no-process self-tests and mutation
rows; and one forward plus one reverse current-pin loopback attempt if all
preflight and failure-capture gates pass.

Out of scope: production transport changes, NTCP2 daemon activation, public
network attempts, reference-source changes, Java-family qualification, full
Plan 434 closure, and support/conformance promotion.

## Ordered work packages

1. Define a closed result schema for preflight failure, process start/readiness
   failure, nonzero helper/launcher exit, missing or ambiguous log evidence,
   cleanup failure, and success. Exclude command output, paths, hashes that
   encode identity-bearing logs, and unbounded diagnostics.
2. Refactor the runner to retain each process's exit category and allowlisted
   stage/reason codes before cleanup. Hash the raw log only in memory; invoke
   the observer before deleting the owned state. Write the sanitized record
   atomically outside the work tree on every terminal branch.
3. Extend `--self-test` with in-memory or temporary-file negative cases for
   each failure class, ensuring raw bytes, identity hashes, message IDs,
   stdout/stderr, and temporary paths never appear in retained JSON. Verify
   cleanup still runs after output-write and observer failures.
4. Rebuild the helper against pristine pinned source; run focused i2pr-interop
   tests, Plan 414 runner/observer self-tests, the existing Plan 410 checker
   and self-test, NTCP2 vectors and historical-boundary checks, tooling
   inventory, global plan uniqueness, planning tests, and `git diff --check`.
5. Only if WP1–4 pass, run one forward and then one reverse loopback attempt.
   Stop immediately after a failed direction. Retain only the sanitized result
   files; remove raw logs and private state.
6. Close Plan 415 with the exact matrix, commands, attempt outcomes, cleanup
   evidence, security review, and fresh unblock audit. If the one-message path
   works, Plan 434 remains the owner for its complete acceptance matrix.

## Failure, restart, and cleanup

No automatic restart or retry. Every child process is owned by the runner and
must be joined or terminated within bounded deadlines. The evidence file is
written before attempt state removal and remains available if cleanup fails.
Any unexpected peer, non-loopback endpoint, pin mismatch, ambiguous log event,
failed evidence write, or cleanup error stops the sequence with a typed result.

## Compatibility and verification

This is non-production qualification tooling. It changes no wire format,
configuration default, dependency, router listener, or advertised capability.
Required commands:

- `cargo fmt --all --check`
- `cargo test --locked -p i2pr-interop --all-targets`
- `cargo build --locked -p i2pr-interop`
- `bash tools/i2pr-interop/reference/i2pd-current/build.sh --i2pd-source-dir target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5 --output-dir target/interop/plan410-i2pd`
- `python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --self-test`
- `python3 tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --self-test`
- `python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test`
- `bash scripts/check-ntcp2-vectors.sh`
- `bash scripts/check-ntcp2-interoperability.sh` (historical-boundary check only)
- `python3 scripts/check-tooling-inventory.py`
- `python3 scripts/check-global-plan-number-uniqueness.py`
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'`
- `git diff --check`
- Exact-pin forward/reverse attempts, only after all earlier gates pass

No workspace routine floor is needed unless production source changes.

## Acceptance and stop conditions

Pass only if failure outcomes are preserved before cleanup, all negative
controls reject unsafe or ambiguous evidence, the bounded forward and reverse
attempts both authenticate and observe one valid DeliveryStatus, and each
attempt's private state and raw log are removed. If either direction fails,
stop with its sanitized evidence and leave Plan 434 blocked. No protocol
success/failure claim or normal-daemon activation follows from partial results.

## Closure evidence

Record the implementation commit, reference/source/helper hashes, negative
mutation table, per-direction attempt count and sanitized digest, process and
cleanup results, exact commands, findings by severity, Plan 434 disposition,
and confirmation that NTCP2 remains disabled and non-advertised.

