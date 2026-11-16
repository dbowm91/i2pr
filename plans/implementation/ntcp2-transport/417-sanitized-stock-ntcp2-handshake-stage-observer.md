# Plan 417 — add sanitized stock NTCP2 handshake-stage observations

Status: **active** — `in-progress-sanitized-stock-ntcp2-handshake-stage-observer`.
Corrective successor to [Plan 416](../../closure/ntcp2-transport/416-status.md).

## Objective

Extend the Plan 414 stock-log observer and Plan 415 failure-safe runner to retain
only bounded, fixed-name counts for pinned i2pd NTCP2 handshake milestones.
After no-process and static gates pass, spend at most one fresh forward attempt;
run reverse only if forward passes. The result should distinguish TCP accept,
SessionRequest processing, SessionCreated processing, SessionConfirmed send,
and authenticated I2NP block handling without patching i2pd or retaining raw
logs.

## Why ready

- Plan 416 traced the launcher and helper failure paths but found no
  i2pr-owned source discrepancy. The missing detail is which stock i2pd
  handshake milestone preceded `TransportManager::IsConnected` timing out.
- Pristine i2pd 2.61.0 logs named debug milestones for SessionRequest,
  SessionCreated, SessionConfirmed, and NTCP2 I2NP block handling. Plan 414
  already established that the I2NP/type-10 logs are downstream of decrypt and
  frame bounds checks.
- Plan 415 provides an atomic sanitized evidence writer and no-process failure
  controls. Plan 417 adds only fixed stage counts to that seam.
- This stays within loopback network-ID-2 qualification; Plan 431's independent
  non-loopback topology is not required.

## Invariants

1. Use pristine pinned i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`; no reference patching or
   instrumentation.
2. Observe only logs under the runner-owned temporary data directory and only
   after the captured baseline offset.
3. Retain allowlisted stage names and bounded counts only. Never retain log
   lines, peer hashes, message IDs, paths, raw output, or full-log digests.
4. Use only `127.0.0.1`, network ID 2, and the explicit loopback topology.
5. One fresh attempt per direction maximum; stop immediately on the first
   failed direction. The normal daemon remains NTCP2-disabled and
   non-advertised.

## Scope

In scope: source-lock stock log milestones; implement a bounded parser for
post-baseline fixed stage tokens; expose only counts in Plan 415 evidence;
negative-test duplicates, malformed/oversized lines, missing milestones,
pre-baseline messages, and privacy redaction; rebuild and run required checks;
then one forward attempt and, only on forward pass, one reverse attempt.

Out of scope: source or binary changes to i2pd, i2pr transport changes, any
protocol fix, live public network tests, daemon activation, Plan 434's full
fragment/malformed/teardown matrix, and support/conformance promotion.

## Ordered work packages

1. Verify the exact pinned-source wording and order for each observed log
   milestone. Use a closed mapping from log tokens to stage enums; never copy a
   log line into retained data.
2. Extend the observer with bounded post-baseline stage counting, exact
   direction/run scoping, malformed and oversized input rejection, and a
   sanitized self-test matrix.
3. Extend the runner to copy only allowlisted stage counts into its existing
   atomic evidence file. Confirm all hashes, identities, message IDs, paths,
   and raw output remain absent from retained JSON.
4. Rebuild the launcher and pristine pinned helper; run runner/observer
   self-tests, current-pin checker and self-test, interop tests, NTCP2 vectors
   and historical boundary checks, tooling inventory, plan uniqueness,
   planning tests, and `git diff --check`.
5. After gates pass, run exactly one forward loopback attempt. Stop on failure.
   Run one reverse attempt only if forward passes. Preserve sanitized stages
   and verify temporary state cleanup.
6. Close with source provenance, mutation table, exact commands, stage counts,
   attempt budget, cleanup status, security review, and a fresh unblock audit.

## Failure, restart, and cleanup

No retry or automatic restart. Child processes remain runner-owned and are
joined or terminated within bounded deadlines. The external evidence record is
written atomically before deleting the private attempt tree. Any unexpected
token, ambiguous stage, evidence-write error, non-loopback endpoint, or cleanup
failure is a typed non-pass and stops the sequence.

## Compatibility and verification

Non-production qualification tooling only; no dependency or product behavior
change is authorized. No routine workspace floor is required absent production
changes. The exact commands are listed in WP4, followed by the bounded attempt
commands only after all gates pass.

## Acceptance and stop conditions

Pass the observer infrastructure only when all negative controls reject unsafe
or ambiguous input and retained evidence contains only the fixed stage counts.
Wire acceptance requires one authenticated and correlated DeliveryStatus in
both directions, with cleanup passed. If either direction fails, stop with the
sanitized evidence. No protocol conclusion or daemon activation follows from a
partial stage trace.

## Closure evidence

Record the implementation commit, exact pinned source lines and digests, parser
mutation rows, exact command results, per-direction outcome/counts, cleanup
evidence, security review, findings by severity, and unblock audit. Do not
retain raw reference output or attempt state.
