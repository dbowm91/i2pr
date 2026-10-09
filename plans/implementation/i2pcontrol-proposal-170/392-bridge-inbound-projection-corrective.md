# Plan 392 — bridge inbound projection corrective

Status: **blocked-publication-coordinator-retry-leak-plan-393**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Plan 391 established that all activated reverse-server inbound roles obtain a
Destination pool registration and receive owner, but bridge projection rejects
every inbound receive id. `DestinationBridge::append_router_inbound_receive`
has two rejection cases: router network state absent or duplicate receive id.
Plan 392 distinguishes these bounded causes and corrects the source lifecycle.

## Objective

Make post-start server inbound roles install atomically across the canonical
Destination pool, receive-owner map, bridge projection, and usable LS2. Preserve
finite Plan 390 registry capacity and generation ownership. Requalify the
exact-pinned reverse NONE lane, proceeding to auth rows only after payload
success.

## Dependencies and evidence

Hard inputs: Plans 380/381 for credential and frozen live-lane contracts;
Plans 385–391 for post-start authority, fair build scheduling, owner-scoped
build stages, aggregate role capacity, and bridge rejection attribution.
Plan 391 closure records exact counters and evidence hashes.

## Invariants

1. Use stock i2pd 2.61.0 at exact SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`, with `MAX_ATTEMPTS=1`.
2. Keep payload traffic on loopback SAM and the live private mesh.
3. Error attribution is typed/coarse and does not expose receive IDs, group
   IDs, peer hashes, addresses, keys, credentials, payloads, or raw logs.
4. Preserve atomic rollback: an inbound registration not represented in the
   bridge and owner map must not remain usable; conversely, an installed lease
   must remain visible to its canonical Destination pool.
5. Preserve Plan 390 bounds, pending limits, fair scheduling, cancellation,
   generation pruning, and shutdown ownership.
6. No transcript, credential, inventory, support, or advertisement changes.

## Ordered work packages

1. Replace the ambiguous bridge append error with typed/coarse reasons, or an
   equivalent bounded diagnostic, and add counters distinguishing missing
   router network state from duplicate receive projection.
2. Run one clean exact-pinned NONE lane. Use the first typed rejection to trace
   generation reconciliation, bridge initialization, and registration order.
3. Add deterministic coverage for post-start generation replacement and
   inbound installation. Include rollback on both failure categories and
   idempotent behavior only if the lifecycle contract proves it is safe.
4. Correct the first failing source transition. Do not simply ignore duplicate
   projections or recreate bridge state without validating identity and
   Destination ownership.
5. Rerun exact-pinned reverse NONE. Only after successful payload return, run
   PSK/DH, authority and three-peer controls, then relevant ELS2 guards and the
   routine floor.

## Verification

Format, managed-app sibling build before focused daemon tests, focused bridge
and service-generation regressions, runner/checker self-tests, mutation table,
encrypted-consumer caller guard, and exact-pinned NONE. The full floor is
conditional on the live matrix passing.

## Acceptance and stop conditions

Pass requires the server to retain at least the minimum usable inbound lease,
install a signed LS2, cross local DatabaseStore admission, and return the
stock NONE payload. Keep the Plan 390 capacity/max+1 lifecycle tests passing.
If bridge installation succeeds but lease derivation or publication exposes a
new independent failure, register a successor plan.

## Lifecycle, compatibility, and security

No persistent format or public wire/control schema changes. Pool registrations
remain canonical for material and expiry. No secret cloning or unbounded state.
The product failure remains confined to service provisioning; no capability,
support, or anonymity claim changes.

## Closure evidence required

Record typed rejection counts, root cause, regression and cleanup behavior,
generation cancellation, exact commands/results, live hashes, secret review,
and registry/roadmap unblock audit.
