# Plan 393 — publication coordinator retry rollback corrective

Status: **blocked-publication-begin-rejection-cause-plan-394**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Plans 390–392 established the reverse server's finite role registrations,
usable inbound leases, and signed LS2. Exact-pinned publication then stopped
at `begin_ls2_publication` after repeated failures. That method reserves a
bounded coordinator entry, but `publish_service_ls2_for_service` does not
cancel the entry when any later compose or delivery step fails. Retries can
consume the eight-entry cap.

## Objective

Make each local LeaseSet publication attempt release its coordinator intent on
every failure before successful completion, preserving successful ACK handling
and existing retry policy. Prove cancellation restores capacity and requalify
the exact-pinned reverse NONE payload.

## Dependencies and evidence

Hard inputs: Plans 380/381 for credentials and frozen lane; Plans 385–392 for
post-start authority, inbound pool fairness, role capacity, staged bridge state,
and LS2 installation. Plan 392 closure records the pinned publication-stage
failure and artifacts.

## Invariants

1. Use unmodified stock i2pd 2.61.0 at exact SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`, with `MAX_ATTEMPTS=1`.
2. Keep payload traffic on loopback SAM and the live private mesh.
3. Every begun publication is either retained for legitimate response/ACK
   tracking after delivery admission or explicitly cancelled on error. Do not
   release an intent that is still needed to match a legitimate reply.
4. Keep publication capacity finite, retries bounded, and cancellation owned
   by the same coordinator.
5. Do not weaken floodfill eligibility, skip LS2 validation, or claim floodfill
   storage/retrieval from local transport admission.
6. No transcript, credential, support, capability, inventory, or advertisement
   change.

## Ordered work packages

1. Trace every fallible exit after `begin_ls2_publication`, including tunnel
   composition, request construction, delivery rejection, and multi-cell
   partial delivery. Decide whether to cancel the intent immediately on each
   error or centralize cleanup in a helper that cannot bypass cancellation.
2. Add a deterministic coordinator regression: fill publication capacity,
   cancel one failed attempt, then admit a replacement; verify successful
   pending intents remain until matching ACK/cancel and capacity is released
   exactly once.
3. Correct production rollback. Preserve the ACK path's existing removal and
   ensure a partial transport admission is cancelled as a failed attempt.
4. Run clean exact-pinned NONE. Only if it returns the reverse payload proceed
   to PSK/DH, authority and three-peer controls, then guards and the routine
   floor.

## Verification

Run formatting, managed-app sibling build before focused daemon tests, focused
publication coordinator and rollback tests, runner/checker self-tests,
mutation table, encrypted-consumer caller guard, and exact-pinned NONE. The
full floor is conditional on successful live acceptance.

## Acceptance and stop conditions

Pass requires a failed publication attempt to leave no orphaned coordinator
entry, a subsequent attempt to be admitted within the configured bound, and
successful admitted publication to retain ACK correlation. The control-created
server must retain usable leases, install LS2, cross local DatabaseStore
admission, and return the stock NONE payload. Later matrix rows are conditional
on that pass.

If all coordinator capacity is available but the live publish still fails at a
new distinct stage, register a successor plan and preserve the exact artifact.

## Lifecycle, compatibility, and security

No persistent format or public API/wire change. Cleanup is explicit and
coordinator-owned; no background retries or unbounded attempt history. Local
transport admission remains distinct from remote storage confirmation. No
capability or anonymity claim changes.

## Closure evidence required

Record every post-begin failure path, capacity/cancel/ACK regression, exact
commands and outcomes, live artifact hashes, lifecycle review, and registry/
roadmap unblock audit.
