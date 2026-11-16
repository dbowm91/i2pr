# Plan 416 — diagnose the current-pin NTCP2 authentication-stage failure

Status: **stopped** — `stopped-source-trace-did-not-localize-an-i2pr-owned-correction`.
Corrective successor to [Plan 415](../../closure/ntcp2-transport/415-status.md).

## Objective

Use Plan 415's sanitized forward result to trace why the pinned i2pd helper did
not observe its expected peer as connected while the i2pr launcher reported
`receiver-frame-read-failed`. Localize the discrepancy to a concrete
i2pr-owned runner/configuration or transport stage before changing code or
spending another wire-attempt budget.

## Why ready

- Plan 415 preserved bounded stage evidence: TCP connected; the launcher then
  rejected the receiver frame; the helper's `TransportManager::IsConnected`
  check did not find its expected peer; no decoded DeliveryStatus was observed.
- The cause is unresolved and has a narrow source-trace boundary across the
  existing Plan 410 helper configuration, i2pr scenario/launcher, and NTCP2
  receiver state transitions.
- Plan 431's independent non-loopback topology is not required for this
  loopback-only source diagnosis.
- Plan 101 remains authoritative: normal-daemon NTCP2 is disabled and
  non-advertised.

## Invariants

1. Use the pinned pristine i2pd 2.61.0 source and existing loopback network-ID-2
  profile. Never patch or instrument the reference router.
2. Do not run a wire attempt until source tracing identifies one concrete,
  i2pr-owned correction and its no-network regression controls pass.
3. Keep every endpoint on `127.0.0.1`; no reseed, public egress, or external
  peer.
4. Retain only fixed stage/reason codes, bounded counters, process outcomes,
  and source/build provenance. Do not retain raw logs, identities, message IDs,
  paths, or raw process output.
5. Preserve the first-failure rule and a maximum of one attempt per direction.
6. Do not activate or advertise daemon NTCP2 and do not change conformance or
  support claims.

## Scope

In scope: trace the helper's peer-hash/configuration path, the launcher
`tcp_connected` → `receiver-frame-read-failed` path, and the transport session
state that feeds the helper's `IsConnected` observation; add regression
controls; make one minimal i2pr-owned fix if and only if source evidence
localizes a defect; and, after all gates, run one forward loopback attempt and
then one reverse attempt only if forward passes.

Out of scope: NTCP2 protocol rewrites, reference-source changes, public or
independently addressed network tests, daemon activation, Plan 434's full
fragment/malformed/teardown matrix, and support/conformance promotion.

## Ordered work packages

1. Re-read Plans 099, 410, 414, and 415 plus the current source and evidence.
   Trace the receiver-frame rejection through exact i2pr source functions and
   the peer identity/config path through the helper's `IsConnected` check.
2. Record a stage-by-stage source comparison and identify the first concrete
   discrepancy. If source tracing does not localize an i2pr-owned correction,
   stop with a typed blocker and do not run a wire attempt.
3. If localized, implement the smallest i2pr-owned correction and add focused
   no-network regression controls that fail against the pre-fix behavior.
4. Rebuild the launcher and pristine pinned helper; run the focused tests,
   Plan 415 runner and observer self-tests, current-pin checker and self-test,
   NTCP2 vectors/historical boundary, tooling inventory, plan uniqueness,
   planning tests, and `git diff --check`.
5. Only if WP1–4 pass and the root cause is localized, run one fresh forward
   attempt. Stop on failure. Run one reverse attempt only if forward passes.
   Preserve the sanitized result and verify temporary state cleanup.
6. Close with the complete source-trace matrix, the regression evidence, exact
   command results, attempt count, cleanup evidence, and an unblock audit.

## Failure and cleanup

No retries, automatic process restarts, or fallback topology. Every process is
joined or terminated within bounded deadlines. Evidence is written outside the
owned temporary directory before cleanup. A failure to localize the cause,
evidence-write failure, unexpected peer, or cleanup failure stops the plan.

## Compatibility and verification

Non-production qualification tooling only. No production behavior or dependency
changes are authorized except a narrowly localized i2pr-owned receiver/config
fix with focused regression controls. No routine workspace floor is required
unless production code is changed. Required commands are those listed in WP4,
plus exact-pin loopback attempts only after the earlier gates pass.

## Acceptance and stop conditions

Pass the source-diagnosis phase only if the failed stage is traced to exact
source and the pre-fix defect is captured by a regression control. Any wire
acceptance requires one authenticated and correlated DeliveryStatus in both
directions, with successful temporary-state cleanup. If the cause remains
ambiguous or either direction fails, stop with sanitized evidence and leave
Plan 434 blocked.

## Closure evidence

Record implementation commits, exact source path and first divergent stage,
regression rows, pristine reference/helper provenance, all verification
commands, any attempt outcome and cleanup, security review, findings by
severity, and the unblock audit. No raw logs or identity-bearing values may be
committed.
