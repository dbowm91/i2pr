# Plan 414 — stock i2pd decoded-message observation for Plan 410

Status: **stopped** — `stopped-forward-attempt-failed-with-unclassified-stage`.
Corrective successor to Plan 410; see `plans/closure/ntcp2-transport/410-status.md`.

## Objective

Determine whether the exact pinned, unmodified i2pd 2.61.0 debug log is an authoritative observation surface for one inbound DeliveryStatus received over the Plan 410 network-ID-2 loopback profile. If the source-to-log path and a single-peer attempt make attribution unambiguous, add a bounded sanitized observation adapter and resume Plan 434's current-pin qualification. If not, record the precise evidence gap without changing the reference or production protocol behavior.

## Why ready

- Plans 430 and 432 are closed; Plan 410 provides the explicit loopback profile and builds a helper against pristine pinned i2pd 2.61.0.
- The exact pinned source contains a candidate diagnostic path: `NTCP2Session::HandleReceived` logs only after AEAD verification; `ProcessNextFrame` bounds the I2NP block and calls `FromNTCP2`; and `I2NPMessagesHandler::PutNextMessage` reaches `HandleI2NPMessage`, which logs the decoded I2NP type. This hypothesis has not yet been used as evidence.
- No unresolved protocol decision is required. Normal-daemon NTCP2 stays disabled and non-advertised under Plan 101.

## Invariants

1. Use only stock i2pd `2.61.0`, revision `635b013a612ff47278ef02acf8580a28e10e26c5`; do not patch, wrap, or rebuild its source with observer instrumentation.
2. Keep all endpoints on `127.0.0.1`, use network ID 2 only through the Plan 410 current-network-loopback profile, and disable reseed and other egress.
3. Freeze `MAX_ATTEMPTS=1` for each direction before any wire attempt. Use fresh state and bounded startup, handshake, data, and shutdown deadlines.
4. Use the exact pinned source call graph to decide whether the log line proves post-decrypt, post-frame-parse I2NP type dispatch. A message-type count alone is insufficient unless the runner proves there is only one eligible peer and one target inbound DeliveryStatus during the observation window.
5. Raw logs are disposable local input only. Do not commit, upload, or include their contents in evidence. Persist only pin, direction, typed stage, bounded counts, result code, and digests.
6. Preserve Plans 099–101, Plan 410, and the historical interop lane unchanged. Do not enable or advertise NTCP2 in `i2pr-daemon` or promote support claims.

## Scope

In scope: source-chain verification at the frozen pin; an adapter that counts the exact type-10 handling line without retaining raw log text; negative tests for absent, malformed, duplicated, unrelated, or pre-window observations; and at most one forward and one reverse loopback attempt if the observation gate passes.

Out of scope: protocol or reference-source changes, public-network attempts, normal-daemon activation, RouterInfo publication, Java qualification, and any floodfill or support-claim change.

## Ordered work packages

1. Record exact source and helper hashes; verify the complete source chain from successful NTCP2 decrypt through the type-10 I2NP dispatch log. Confirm logger destination and lifecycle for the Plan 410 helper.
2. Specify the admissible attribution window. Verify that one target session and exactly one inbound DeliveryStatus are possible, and that background startup traffic cannot satisfy the observation. If this cannot be proven, stop before wire execution.
3. Add a small parser/runner adapter that reads the ephemeral log after the process exits, emits sanitized evidence, rejects ambiguous matches, and deletes the raw log and fresh state on every terminal path. Add self-tests that start no reference process and negative-test the parser and sanitizer.
4. Run one forward and one reverse attempt with `MAX_ATTEMPTS=1`. Require authenticated transport plus one decoded DeliveryStatus in each direction; correlate the reverse i2pd emission with the i2pr receive event. Preserve only sanitized digests and counts.
5. Run focused i2pr-interop tests, the new adapter self-test and evidence checker (including mutations), NTCP2 vector and boundary checks, tooling inventory, global plan-number uniqueness, planning tests, and `git diff --check`. Run the routine floor only if production source changes.
6. Close this plan with the evidence matrix, exact commands and outcomes, security and cleanup review, and a fresh unblock audit for Plans 434/435.

## Failure and cleanup

Any pin mismatch, non-loopback endpoint, unexpected peer/session, ambiguous or missing type-10 line, exceeded deadline, or cleanup failure is a non-passing typed result. No retries are allowed. Always stop and join owned processes, remove ephemeral logs and private state, and retain only sanitized evidence.

## Compatibility and verification

This is non-production diagnostic infrastructure. It changes no wire format, configuration default, dependency, or advertised capability. Required commands:

- `cargo test --locked -p i2pr-interop --all-targets`
- new observation adapter `--self-test` and checker `--self-test`
- `bash scripts/check-ntcp2-vectors.sh`
- `bash scripts/check-ntcp2-interoperability.sh` (historical-boundary check only, not Plan 414 evidence)
- exact-pinned forward and reverse loopback attempts, if and only if WP2 passes
- `python3 scripts/check-tooling-inventory.py`
- `python3 scripts/check-global-plan-number-uniqueness.py`
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'`
- `git diff --check`

## Acceptance and stop conditions

Pass only if the pinned source chain is verified, attribution is unambiguous, negative controls reject invalid evidence, both bounded directions authenticate and deliver/receive a valid DeliveryStatus, all processes and state are cleaned up, and evidence contains no raw log or identity-bearing values. Otherwise close as blocked or stopped with the exact failing predicate. Do not rewrite Plan 410's closure record; update Plan 434 only through its own subsequent closure record once its full acceptance criteria are met.

## Closure evidence

Record commit, pinned source/helper hashes, source line references, parser and mutation results, each bounded attempt and sanitized digest, process/state cleanup outcomes, exact commands, findings by severity, Plan 434/435 readiness, and the fact that normal-daemon NTCP2 remains disabled and non-advertised.

