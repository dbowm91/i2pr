# Plan 401 — reverse authorized ELS2 lookup attribution

Status: **in-progress-reverse-authorized-els2-lookup-attribution**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Attribute why the stock i2pd 2.61.0 requester reports `LeaseSet not found` for
reverse PSK after local type-5 publication admission, using bounded source and
runtime evidence. Determine whether Plan 400's DH row can run on a healthy,
meaningful control. Do not change production behavior until evidence identifies an
i2pr-owned defect and a narrowly scoped correction.

## Why ready

Plan 400 passes the reverse NONE row with an explicit type-7 transient SAM
destination. Its PSK row passes the reference mesh and i2pr ordinary authority
control but ends before any inbound Garlic transaction: i2pd reports
`CANT_REACH_PEER / LeaseSet not found`. The existing evidence cannot distinguish a
missing type-5 floodfill record from authorization rejection or another lookup
failure. Plan 399 establishes the preceding inbound signature-profile boundary.
Plan 401 isolates the authorized lookup stage and preserves the frozen i2pd pin.

## Invariants

1. Use stock i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`; one attempt per lane invocation.
2. Keep reverse requester signature type 7 explicit. No type-0 support.
3. No raw logs, credentials, keys, addresses, IDs, or payloads in artifacts.
4. Do not alter type-11 transcript, Proposal field inventory, crypto profile,
   support/conformance, or advertisement.
5. Keep production code unchanged unless source and bounded evidence establish an
   i2pr defect within this plan; if a corrective is needed, register a successor
   before the production edit.
6. A failed control invalidates that run; preserve it and do not retry in-place.

## Scope

In scope: inspect exact-pin i2pd SAM, LeaseSet lookup, and ELS2 authorization paths;
verify the lane's PSK/DH parameter construction against the pinned source; add only
bounded transaction-stage evidence necessary to distinguish publication/store,
lookup, authorization, and route outcomes; execute one PSK attribution attempt and,
only when controls are healthy, one DH attempt. Record whether Plan 400's NONE
qualification remains valid under its explicit profile.

Out of scope: changing i2pd, adding legacy signature support or dependencies,
changing crypto/transcripts, broad Java qualification, changing Proposal 170 fields,
or promoting support/capability claims.

## Work packages

1. Establish pinned source availability and inspect the relevant request and
   authorization code. If source is unavailable locally, use exact-pin upstream
   source citations and record that provenance; do not infer an unsupported path.
2. Trace sanitized driver states from publication accepted through NetDB storage,
   client lookup, authorization, Garlic receipt, and payload delivery. Add bounded
   counters/enums only if existing evidence cannot identify the transition.
3. Confirm the reverse PSK/DH setup uses the intended typed credentials and that
   the stock client receives the proper mode-specific credential without exposing
   it. Add focused guards for any new evidence surface.
4. Run healthy controls and exactly one PSK attempt. Run one DH attempt only if the
   PSK evidence or common lookup control demonstrates the same attribution is
   meaningful and all authority controls pass.
5. If evidence localizes an i2pr-owned implementation defect, register a corrective
   successor before changing production behavior. Otherwise close with a precise
   external/reference limitation and identify the next required evidence owner.

## Failure and compatibility

No persisted data changes. Instrumentation is bounded, transaction-scoped, and
privacy-safe. If instrumentation itself cannot distinguish a stage without exposing
secret or raw routing material, stop and record the gap. A source-level conclusion
must cite the exact pinned source path and symbol.

## Verification

- `cargo fmt --all --check`
- Build managed-app siblings before any focused daemon test.
- Focused Rust tests for any transaction-stage evidence and its saturation/drop
  behavior.
- ELS2 live-lane checker, runner self-test, evidence guard, and encrypted-consumer
  caller guard.
- Exact-pinned controls plus one PSK row; at most one DH row under the condition
  above, with `MAX_ATTEMPTS=1`.
- Full routine floor only after the required live matrix is complete.

## Acceptance

The PSK `LeaseSet not found` result is attributed to a bounded stage using pinned
source and sanitized runtime evidence, or is explicitly classified as unresolved
at a named reference boundary. Any i2pr defect has a separately registered
corrective before implementation. DH is run only with valid controls and meaningful
diagnostic value. No failed attempt is discarded and no unsupported compatibility
claim is added.

## Stop conditions

Stop before behavior changes if source/evidence shows the request cannot present the
required credential, if the healthy mesh or authority controls fail, if attribution
requires raw secret/routing data, or if a fix would widen protocol/crypto scope.
Register a successor for any newly localized implementation defect.

## Closure evidence required

Record exact source pin and symbols, per-control and per-mode result, sanitized
artifact hashes, commands and outcomes, evidence-stage interpretation, any
instrumentation, security/compatibility review, unresolved findings, and the
registry/roadmap unblock audit. State explicitly which Plan 400 rows remain
unqualified.
