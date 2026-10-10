# Plan 418 — enable stock debug-stage logging in the current-pin helper

Status: **stopped** — `stopped-forward-session-confirmed-received-without-connected-peer`.
Corrective successor to [Plan 417](../../closure/ntcp2-transport/417-status.md).

## Objective

Configure the i2pr-owned direct helper to enable the pinned i2pd logger's
existing debug output before starting the logger. Add a fail-closed source
guard and no-network controls, then run at most one fresh forward attempt and
one reverse attempt only if forward passes. No i2pd source or binary is
modified.

## Why ready

- Plan 417's sanitized forward evidence retained zero post-baseline debug
  milestones and no raw log.
- Pinned i2pd 2.61.0 source shows `Log::m_MinLevel` defaults to `eLogInfo`; the
  NTCP2 SessionRequest, SessionCreated, SessionConfirmed, decrypt, and I2NP
  milestones used by the observer are `eLogDebug`.
- The i2pr-owned helper starts the logger after configuring its output path but
  does not call the public `Logger().SetLogLevel("debug")` API.
- Enabling an existing log level in the isolated helper process is bounded to
  non-production evidence tooling and preserves the no-reference-patching
  rule.

## Invariants

1. Do not modify, patch, or instrument pinned i2pd source or binary. The helper
   remains separately built i2pr-owned tooling linked to pristine libraries.
2. Set debug level only in the private helper process, before that process
   starts the logger; never change normal daemon logging or configuration.
3. Preserve the Plan 417 post-baseline parser and sanitized evidence schema.
   Raw log lines, message IDs, Router Hashes, paths, process output, and log
   digests remain excluded.
4. Use only `127.0.0.1`, network ID 2, and `current-network-loopback`.
5. One attempt per direction maximum; stop after the first failure. No public
   peer, reseed, egress, daemon activation, or capability advertisement.

## Scope

In scope: configure logger debug level before `Logger().Start()` in the owned
helper, extend the current-pin checker with a source-order assertion and
negative mutation, add focused no-network controls, rebuild against pristine
pinned i2pd, run all required gates, then one forward attempt and reverse only
if forward passes.

Out of scope: reference-router changes, production Rust transport changes,
NTCP2 daemon activation, public-network tests, Plan 434's full
fragment/malformed/teardown matrix, and conformance/support promotion.

## Ordered work packages

1. Confirm the pinned log default and the exact debug-level log sites from
   pristine source. Keep their interpretation limited to stage observation.
2. Call the public logger level setter before logger startup in the helper.
   Ensure inspect and active modes retain bounded owned-state behavior.
3. Extend `check-current-pin-ntcp2-runner.py` so it fails if the call is absent
   or ordered after `Logger().Start()`. Add a negative mutation for each case.
4. Extend no-process tests to confirm the helper config assertion, post-baseline
   stage counts, no raw log output, and cleanup after classification.
5. Rebuild the i2pr interop launcher and helper against pristine pinned i2pd;
   run focused interop tests, both self-tests, checker and self-test, NTCP2
   vectors/historical boundary, tooling inventory, plan uniqueness, planning
   tests, and `git diff --check`.
6. After all gates pass, run one forward attempt. Stop on failure. Run reverse
   only if forward passes. Retain only sanitized counts and verify cleanup.
7. Close with exact commands/results, mutation matrix, attempt outcome, cleanup,
   security review, and unblock audit.

## Failure and cleanup

No retry or fallback. All child processes are joined or terminated within
bounded deadlines. The sanitized evidence record is written atomically outside
the private work tree before cleanup. If source-order checks, evidence writing,
loopback scoping, or cleanup fail, stop with a typed non-pass.

## Compatibility and verification

Non-production tooling only. The debug-level setting belongs only to the
isolated helper, does not alter the host's i2pd or router daemon, and produces
no retained log text. No dependency changes. Routine workspace floor is not
required unless production code is touched.

## Acceptance and stop conditions

Pass the configuration gate only if the checker proves debug logging is
enabled before logger startup and mutations fail closed. The wire lane passes
only if both directions authenticate, observe exactly one correlated
DeliveryStatus, and clean all attempt state. Any failed direction stops the
sequence and leaves Plan 434 blocked; no protocol claim or daemon activation
follows from a partial result.

## Closure evidence

Record implementation commits, pristine source/helper hashes, the logging
configuration guard and mutation rows, exact commands, per-direction sanitized
counts/results, cleanup, security review, findings by severity, and a fresh
unblock audit. Never commit raw logs or identity-bearing values.
