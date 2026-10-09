# Plan 406 — authority stream-stage and final matrix corrective

Status: **in-progress-authority-stream-stage-and-final-matrix-corrective**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Attribute why the final Plan 405 NONE run resolved the reference standard LS2
but returned no application payload, then finish the Plan 384 final NONE/PSK/DH
matrix and routine floor once the authority boundary is demonstrated on a
healthy reference mesh.

## Why ready

Plan 405's second final NONE artifact passed the stock reference ELS2 self-test
and standard self-consumer control. The i2pr ordinary lookup succeeded and the
encrypted-target status resolved, but the payload row failed with no recorded
Streaming connect start, no failed connect, and no activation failure. A stock
i2pd process also exited abnormally during cleanup; earlier Plan 400/403/404
artifacts passed the corresponding authority paths. The current evidence cannot
distinguish a local listener acceptance gap, an unstarted Streaming attempt, or
a remote process that stopped after lookup. Plan 406 adds bounded stage evidence
and health checks before another attempt.

## Invariants

1. Stock i2pd stays at `635b013a612ff47278ef02acf8580a28e10e26c5`; never patch it.
2. Each lane invocation retains `MAX_ATTEMPTS=1` and a unique artifact directory.
3. Reference process health gates the i2pr driver; dead processes invalidate the
   attempt and cannot be reported as product evidence.
4. New evidence is limited to named process `alive/dead`, active-connection
   counts, bounded phase enums, and existing counters. Never record PIDs,
   addresses, IDs, secrets, logs, or payload bytes.
5. Preserve all prior pass and failure artifacts. No reclassification or deletion.
6. No production behavior changes until evidence proves an i2pr-owned defect.
7. No transcript, support, conformance, or advertisement change.

## Scope

In scope: add a sanitized reference-process-health row immediately before and
after the driver; ensure a pre-driver dead peer fails closed without running the
product test; sample the authority client's bounded active-connection count
during the payload window; retain activation status and counters; run a fresh
NONE diagnostic attempt, then PSK and DH only after the authority row passes;
run the AGENTS.md floor after the complete matrix passes.

Out of scope: Java I2P, proposal-wide convergence, unsupported modes, production
transport changes without a registered successor, and broad retry-budget
changes.

## Work packages

1. Extend the runner with named-reference-process liveness evidence using its
   existing PID files; do not emit numeric PIDs. Fail closed before the i2pr
   test if any required process is absent or zombie.
2. Extend the live test's authority read helper to report a saturating peak
   active-connection count for the ordinary authority service during the
   existing bounded payload window. Reuse `active_connections`; do not add a
   new production counter.
3. Guard the health row, active-connection evidence, mode configuration, and
   absence of secrets with negative checker mutations.
4. Run a fresh NONE lane. If the authority payload passes, run PSK and DH once
   each with separate evidence. If it fails on a process-health row, preserve
   that control failure; if all processes are healthy and a product transition
   is missing, register a successor before behavior changes.
5. After NONE/PSK/DH payload and authority controls pass, execute every routine
   floor command in AGENTS.md and record exact outcomes.

## Failure and compatibility

The new instrumentation is test-only and uses existing read-only manager
accessors. Process health is a prerequisite, not a retry trigger. Each lane is
one attempt; failures remain artifacts. A behavior correction requires a new
plan before editing production behavior.

## Verification

- ELS2 Python checker, runner self-test, live evidence guard, encrypted-consumer
  caller guard, and formatting.
- Focused `cargo check --locked -p i2pr-daemon --test els2_i2pd_external`.
- One fresh exact-pinned NONE diagnostic run; then PSK and DH only after authority
  succeeds, all with `MAX_ATTEMPTS=1`.
- Full AGENTS.md routine floor after all live rows pass.

## Acceptance

The process-health gate rejects unhealthy reference meshes before driver
execution; authority evidence identifies whether the local connection was
accepted; NONE, PSK, and DH each pass the ordinary authority and reverse payload
rows on healthy exact-pinned meshes; and the full routine floor passes. The
Plan 404 anomaly and Plan 405 failed authority artifact remain recorded.

## Closure evidence required

Record health and activity evidence, all controls and payload rows, hashes for
every attempt, exact floor command outcomes, known anomalies, security and
compatibility review, and the unblock audit for Plans 374/375/377/378 and 384.
