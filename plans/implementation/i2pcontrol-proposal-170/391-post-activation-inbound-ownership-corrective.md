# Plan 391 — post-activation inbound ownership corrective

Status: **blocked-bridge-inbound-projection-rejection-plan-392**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Plan 390 corrected the shared role-registry capacity: the exact-pinned NONE
lane now activates 33 inbound and 3 outbound Destination builds with no
registration-stage failures. Yet the control-created server has zero live
inbound pool registrations and no LS2. The next boundary is after role
activation, where inbound receive ownership and bridge projection are
installed.

## Objective

Add bounded Destination-scoped attribution for post-activation inbound
retention, receive-owner registration, bridge projection, and usable lease
derivation. Localize and correct the first failing transition while preserving
Plan 390's finite registry capacity, generation cancellation, and existing
service ownership. Requalify exact-pinned NONE before continuing the auth
matrix.

## Dependencies and evidence

Hard inputs: Plans 380/381 for the frozen credential and pinned live lane;
Plans 385–390 for post-start authority, reverse publication, server readiness,
fair build scheduling, owner-scoped registration stages, and aggregate role
capacity. Plan 390 closure records the post-activation evidence and hashes.

## Invariants

1. Use unmodified stock i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5` and `MAX_ATTEMPTS=1`.
2. Keep payload traffic on loopback SAM and the live private mesh.
3. Diagnostics remain bounded, numeric/coarse, and Destination-owner scoped;
   never include peer hashes, addresses, role/tunnel ids, attempt ids, keys,
   credentials, LS2 bytes, payloads, or raw reference logs.
4. A retained inbound role must correspond to a registration in the owning
   `DestinationRuntime`, a registered receive owner, and the bridge's inbound
   receive projection. Any partial installation rolls back atomically.
5. Preserve Plan 390 capacity derivation, one coordinator, pending bounds,
   fair scheduling, generation cancellation, and shutdown ownership.
6. Do not alter transcript semantics, the Plan 380 credential seam, Proposal
   170 inventory, support inventory, or advertisement posture.

## Ordered work packages

1. Add bounded counters separating: successful role activation; pool slot and
   receive-id lookup; inbound owner registration; bridge append; cleanup/rollback;
   surviving pool registration; minimum usable lease derivation; LS2 installation.
   Counters remain current-generation-only and are pruned with their owner.
2. Run one clean exact-pinned NONE diagnostic. Use evidence to identify the
   first failed transition, including whether repeated receive ids or a
   coordinator/bridge ownership mismatch is involved. Do not record identifiers
   in evidence.
3. Trace the source owner and add a deterministic regression that exercises
   the failing post-activation transition, max+1/cleanup where relevant, and
   generation replacement cancellation.
4. Correct only the identified transition. Successful installation must leave
   a usable registration visible to the same Destination pool that derives the
   signed Standard LS2.
5. Re-run exact-pinned reverse NONE. Only after NONE returns the stock payload,
   run PSK/DH, the authority payload, and three-peer controls, followed by the
   ELS2 guards and routine floor.

## Verification

Run formatting, managed-app sibling build before focused daemon tests, focused
Destination/registry regressions, runner and evidence checker self-tests,
mutation table, encrypted-consumer caller guard, and exact-pinned NONE.
Full workspace qualification is conditional on successful live matrix results.

## Acceptance and stop conditions

Pass requires the control-created server to retain at least the configured
minimum inbound leases, install its signed LS2, cross local DatabaseStore
admission, and return the NONE payload. Preserve all Plan 390 capacity and
max+1 lifecycle evidence. If role activation succeeds but an additional
distinct owner boundary fails, register a further corrective instead of
collapsing the stages.

## Lifecycle, compatibility, and security

No persistent format or public wire/control schema changes. Pool registrations
remain the canonical owner of material and expiry. No secret cloning, raw
error strings, or unbounded diagnostic history. Product failure remains
confined to service tunnel provisioning. No support, capability, or anonymity
claim changes.

## Closure evidence required

Record stage counts and root cause, source and regression, generation
cancellation, role cleanup and secret review, exact commands and outcomes,
hashes for all executed rows, and the plan registry/roadmap unblock audit.
