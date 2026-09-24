# Plan 248 — Interoperability evidence-policy reconciliation and M11 handoff

Status at registration: owner-directed planning reconciliation; executed and closed in
this documentation-only authority transition.

## Objective

Apply ADR 0026 to live planning/canonical surfaces without altering historical execution
evidence, then register the first dependency-ready M11 transit-tunnel implementation plan.

No Rust production code, test harness behavior, reference pin, or recorded test result may
change in this plan.

## Why ready

- M6 local authority is closed.
- Plan 193 exact-pinned i2pd mixed-router Streaming qualification is closed with two
  complete bidirectional passes.
- Plan 215 is the closed M10 product authority.
- Plan 247 is closed and records no production i2pr defect.
- ADR 0026 records the maintainer decision separating progression from full conformance.

## Required changes

1. Update specs/CONFORMANCE.md with explicit evidence tiers while retaining the two-family
   full router-to-router requirement.
2. Amend Plan 201 to retained/deferred nonblocking Java compatibility debt.
3. Supersede Plan 204's Java-dependent convergence gate without pretending it executed.
4. Reconcile registry, M6/M10 roadmaps, README, support inventory, architecture prose, and
   tunnel dossier.
5. Create the M11 transit-tunnel roadmap.
6. Register Plan 249 as the sole dependency-ready M11 implementation plan.

## Invariants

- No historical result is relabeled.
- No Java defect or Java pass is invented.
- No full two-family M6 conformance claim is created.
- No public-network or production-readiness claim is created.
- No RouterInfo capability, router.version, transport advertisement, daemon exposure, or
  reference pin changes.
- Plan 193 remains external progression authority.
- Plan 215 remains M10 product authority.

## Out of scope

Running Plan 247; deleting Java helpers; implementing transit participation; enabling
transit/floodfill/public I2P; changing resource/security defaults.

## Verification

This is a documentation/planning transition. Verify structurally that all new
plan/status/roadmap paths exist; registry and roadmaps agree; specs/CONFORMANCE.md retains
the full two-family router rule; Plan 249 is the only new ready implementation plan; and no
files under crates/, tests/, tools/, scripts/, .github/, Cargo.toml, or Cargo.lock change.

## Acceptance

Plan 248 closes only when ADR 0026 is accepted, Plan 201 is retained/deferred, Plan 204 is
superseded for live dependency purposes, M6 experimental progression is recorded via Plan
193, full two-family conformance remains unclaimed, and Plan 249 is registered ready.

## Stop conditions

Stop instead of normalizing authority if canonical requirements are found to mandate Java
full-router Streaming specifically for experimental progression or if Plan 193 lacks a
complete independent bidirectional external lane. Neither condition was found.

## Closure evidence

The status record must state that this is docs/planning-only, preserve historical
execution, and record the unblock audit moving M11 / Plan 249 to ready.
