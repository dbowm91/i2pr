# Plan 388 — post-start ELS2 pool submission and completion corrective

Status: **blocked-post-start-inbound-builds-stall-before-response-plan-389**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Corrects Plan 387's unresolved local provisioning stage. A control-created
server is visible to the product's committed-generation sweep and publication
queue, but an exact-pinned run still observed zero inbound/outbound
registrations, zero usable inbound leases, and no LS2. The current redacted
snapshot has been extended to expose per-destination pending-build and bounded
failure/pause state plus coarse coordinator counters. A fresh clean run must
capture these before naming the faulty transition.

## Objective

Localize and correct why a server Destination committed after product startup
does not submit or complete its router-backed inbound/outbound builds, install
its signed LS2, and reach the existing type-5 publication handoff. Do not
assume the failure is pool selection, coordinator submission, peer response,
installation, or publication until evidence identifies it.

## Dependencies and evidence

Hard/interface inputs: Plans 380 and 381; Plan 385's post-start authority
regression; Plans 386 and 387 blocked evidence; Plan 387's expanded redacted
provisioning snapshot. The service generation, destination pool, build
coordinator, and type-5 publication contracts are already written and stable.

## Invariants

1. Use unmodified stock i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`.
2. Keep `MAX_ATTEMPTS=1`; retries inside bounded pool policy must not become
   remote lookup retries.
3. Every payload crosses loopback SAM and the live I2P mesh.
4. Preserve bounded owner-tracked builds/publication, generation cancellation,
   and the existing retry/failure limits. Do not reset a paused pool or
   silently retry without a reviewed lifecycle policy.
5. Keep Plan 346 transcript, Plan 380 credential seam, Proposal 170 inventory,
   support inventory, and advertisement posture unchanged.
6. Evidence contains only sanitized outcomes, coarse counters, and hashes.

## Ordered work packages

1. Run one clean exact-pinned NONE diagnostic with a unique absolute evidence
   directory. Use the per-group pending counts, failure/pause state, and
   coordinator counters to classify the boundary: group discovery, build
   submission, peer session/build completion, pool registration, lease
   derivation/install, or publication.
2. Trace the identified owner transition in source and add the smallest
   regression test that fails before the fix. Include control-created server
   creation, generation reconciliation, pending build cancellation on a second
   reconcile, and eventual LS2 installation/publication scheduling.
3. Fix only that transition. Preserve failure isolation, bounded concurrency,
   and cancellation on service-generation replacement/shutdown. Add no second
   listener, pool, identity, or publication owner.
4. Run the exact pinned reverse NONE/PSK/DH matrix with one lookup attempt per
   mode, plus the post-start authority payload and three-peer mesh controls.
5. Run ELS2 boundary/evidence guards and the routine floor only after every
   live row passes. Close with exact evidence hashes and the unblock audit.

## Verification

Before focused daemon tests, run the AGENTS.md managed-app sibling build. Run
formatting, the focused ELS2 black-box/product regressions, the runner
self-test, evidence checker self-test and mutation table, encrypted-consumer
caller guard, and a clean exact-pinned NONE diagnostic. Run PSK/DH and the
complete routine floor only after NONE proves local publication and payload
return.

## Acceptance and stop conditions

Pass requires a control-created server to reach configured pool thresholds,
install a real signed LS2, cross local DatabaseStore delivery admission, and
return the pinned i2pd payload for NONE, PSK, and DH. The post-start authority
row, three-peer controls, checker mutations, ELS2 guards, and routine floor
must pass.

Stop and register a further corrective if the fix needs transcript, inventory,
credential-seam, or advertisement changes; the pinned topology cannot reach
the existing pool threshold; or a clean exact-pinned diagnostic cannot be
obtained. Reference process crashes are recorded as environment failures, not
as product passes or failures.

## Lifecycle, compatibility, and security

New builds remain owned by the product's sole coordinator and are cancelled
when their Destination generation is replaced or the product shuts down. A
server may remain locally bound while pool readiness is pending, but it cannot
be reported published or remotely reachable before the real LS2 exists and
local delivery admission succeeds. No persistent format, control schema,
credential flow, or public support claim changes. Keep identifiers, keys,
payloads, and private control material out of diagnostics.

## Closure evidence required

Record the Plan 387 diagnostic hash and the clean counters that localize the
failure; the source transition and regression; exact commands and outcomes;
live evidence hashes for every executed row; bounded lifecycle/cancellation,
compatibility, and secret review; findings; and registry/roadmap unblock audit.
