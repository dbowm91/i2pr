# Plan 405 — final i2pd ELS2 reverse and authority qualification

Status: **in-progress-final-i2pd-els2-reverse-and-authority-qualification**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Complete the blocked Plan 384 scope using the corrected and guarded stock-i2pd
requester: rerun the exact-pinned NONE/PSK/DH reverse matrix and ordinary
post-start authority control, then run the full repository routine acceptance
floor. This plan is the final execution gate for Plan 384's i2pd direction.

## Why ready

Plans 385–404 localized and corrected the reverse path through post-start
publication, owner routing, sender profile, and requester auth configuration.
The final requester now selects SAM signature type 7 and passes authorized
consumer secrets through pinned i2pd's `i2cp.leaseSetPrivKey`; publisher-only
auth options are excluded. Plan 403 produced a passing PSK reverse row. Plan 404
produced a passing DH authority, consumer, and reverse row with bounded stage
evidence. Plan 400 already recorded a passing NONE row. Plan 404 also retains
one unexplained DH failure followed by a passing exact-pin run; this plan
preserves that anomaly while taking one clean attempt per matrix mode.

## Invariants

1. Keep stock i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`; no source or binary changes.
2. Keep `MAX_ATTEMPTS=1` per runner invocation and separate sanitized artifacts.
3. Use explicit signature type 7 for every reverse requester row.
4. Keep secrets and raw reference logs out of committed evidence.
5. No transcript, crypto profile, Proposal inventory, support, conformance, or
   advertisement changes.
6. Preserve the retained Plan 404 failure as a limitation; do not replace it
   with the passing runs.

## Scope

In scope: run exact-pinned NONE, PSK, and DH lanes using final code; require
reference mesh/authority controls and both payload directions; run the complete
AGENTS.md floor after all live rows pass; update Plan 384, Plan 374's i2pd
remainder, and the affected roadmaps/registry based on evidence.

Out of scope: Java I2P (Plan 375), cross-family convergence (377), final
Proposal-170 conformity (378), new modes or negatives beyond Plan 384's scope,
and all support/capability promotion.

## Work packages

1. Re-run checker, self-test, caller guard, formatting, and required fixture
   sibling build before any focused daemon qualification.
2. Run separate exact-pinned NONE, PSK, and DH invocations, each with one
   attempt and a unique absolute evidence directory. Stop if mesh or authority
   controls fail; preserve all outputs.
3. Verify the final artifacts' row outcomes, credential scrubs, and SHA-256
   hashes. Confirm reverse payload deltas and explicit signature type 7.
4. Run every AGENTS.md routine-floor command and record exact local outcomes,
   including host-specific exclusions only where AGENTS.md defines them.
5. Write a closure record mapping Plan 384 criteria to final evidence; retain the
   Plan 404 anomaly and explicitly keep Plans 375/377/378 blocked where their
   independent dependencies remain open.

## Failure and compatibility

Any failed live row keeps this plan blocked and requires a new corrective
successor before behavior changes. Full-floor failures are reported exactly;
they are not repaired by weakening a checker. No persisted format changes.

## Verification

- Final NONE/PSK/DH exact-pinned lanes with `MAX_ATTEMPTS=1`.
- Full routine floor verbatim from the user-supplied `AGENTS.md`.
- Re-run plan-number uniqueness, workflow validity, live-lane evidence,
  encrypted-consumer, ELS2 transcript, and existing Proposal 170 evidence guards
  included in that floor.

## Acceptance

The final three live runs pass every applicable control and payload row, both
authority and reverse, with explicit type 7 and credential scrubs. Every
routine-floor command passes or has an explicit AGENTS.md host limitation. The
closure records Plan 384's scope complete, Plan 374's i2pd remainder delivered
as scoped by Plan 384, and the independent Java/convergence blockers unchanged.

## Closure evidence required

Record all three run artifacts and hashes, exact commands/results from the full
floor, requirement-to-evidence mapping, source/profile statement, security and
compatibility review, retained Plan 404 anomaly, commit IDs, registry/roadmap
disposition, and unblock audit.
