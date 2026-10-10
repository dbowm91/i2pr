# Plan 419 — observe SessionConfirmed rejection stages safely

Status: **ready** — `registered-session-confirmed-rejection-stage-observation`.
Corrective successor to [Plan 418](../../closure/ntcp2-transport/418-status.md).

## Objective

Extend the pinned stock-log observer with fixed, bounded counts for the exact
SessionConfirmed validation failures found in pristine i2pd 2.61.0 source.
Retain only those counts and existing handshake stages, then run at most one
fresh forward loopback attempt. Run reverse only if forward passes.

## Why ready

- Plan 418 preserved one SessionRequest and one SessionConfirmed receive marker,
  but no authenticated peer or I2NP block.
- Pinned source logs the SessionConfirmed receive marker before validating both
  AEAD parts, the embedded RouterInfo, its freshness and address, and the peer
  static key. The attempt needs fixed-category counts for failures along that
  path.
- The debug level is now enabled by the i2pr-owned helper before the logger
  starts. Plan 417's baseline-filtered observer and Plan 415's failure-safe
  writer are in place.
- Scope remains loopback and network ID 2; no independent non-loopback topology
  is required.

## Invariants

1. Use pristine pinned i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`. Never patch or instrument the
   reference source or binary.
2. Add only exact, source-verified marker patterns. Count matches after the
   captured baseline and never retain complete lines or dynamic suffixes.
3. Retain no Router Hashes, endpoints, message IDs, paths, log hashes, raw
   output, or identity-bearing values.
4. Use only `127.0.0.1`, network ID 2, and `current-network-loopback`.
5. One forward attempt maximum; one reverse only if forward passes. Stop on the
   first failure; no retry.
6. Keep normal-daemon NTCP2 disabled and non-advertised.

## Scope

In scope: add allowlisted counts for SessionConfirmed part-1/part-2 AEAD
failures, unexpected block, RouterInfo signature/freshness/import/address
failures, host mismatch, and static-key mismatch; add no-process mutations and
redaction checks; rebuild and run required gates; and one bounded forward
attempt.

Out of scope: reference source changes, production Rust transport changes,
wire-protocol changes, public-network attempts, daemon activation, Plan 434's
full fragmentation/malformed/teardown matrix, and conformance/support
promotion.

## Ordered work packages

1. Source-lock every selected diagnostic marker to pristine i2pd 2.61.0 and
   confirm it is emitted only at the intended bounded failure site.
2. Extend the parser with closed counts and bounded line handling. Dynamic text
   is matched only as part of the pattern and is never copied to evidence.
3. Extend observer and runner self-tests for each failure counter, duplicate
   and malformed markers, baseline exclusion, count bounds, raw-line and
   identity redaction, and evidence cleanup.
4. Rebuild the launcher and helper against pristine pinned i2pd. Run focused
   interop tests, runner/observer self-tests, the current-pin checker and
   self-test, NTCP2 vectors and historical boundary, tooling inventory, plan
   uniqueness, planning tests, and `git diff --check`.
5. Only after all gates pass, run one forward loopback attempt and retain its
   sanitized stage counts. Stop on failure; run reverse only after forward
   passes.
6. Close with source-marker mapping, mutation matrix, exact command results,
   attempt and cleanup status, security review, and unblock audit.

## Failure, cleanup, and acceptance

No retries or fallback topology. Evidence is atomically written outside the
private attempt directory before cleanup. Any unrecognized or ambiguous marker,
evidence-write failure, non-loopback endpoint, or cleanup failure stops the
plan. A wire pass requires exactly one correlated DeliveryStatus and successful
cleanup in both directions; a partial trace leaves Plan 434 blocked.

## Compatibility and closure evidence

This is non-production qualification tooling with no dependency change. Record
the exact source mapping and hashes, self-test mutations, commands, bounded
stage counts, attempt outcome, cleanup evidence, findings by severity, and the
fresh unblock audit. Never commit raw reference logs or identity-bearing data.
