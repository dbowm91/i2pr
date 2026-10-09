# Plan 404 — authorized DH consumer activation attribution

Status: **in-progress-authorized-dh-consumer-activation-attribution**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Attribute the Plan 403 DH failure where a valid-looking i2pr `.b33` client is
created, but its fixture request fails before encrypted-target status or lookup
counters advance. Determine whether the cause is a test credential mismatch,
reference publication/profile, or an i2pr activation boundary. Do not change
production behavior until evidence localizes an i2pr defect.

## Why ready

Plan 403's exact-pinned DH run passed the three-peer mesh and ordinary authority
controls, and the i2pd reverse requester was configured with the source-proven
`i2cp.leaseSetPrivKey` parameter. The reverse DH row did not complete. In the
separate i2pr consumer direction, the sanitized evidence shows a configured DH
credential, a created B33 client, then no encrypted target status or remote lookup
counters; its failed-connect count advanced once. The one i2pd process abort was
reported during teardown and does not establish the cause. Plan 404 uses bounded
per-stage evidence and source review to identify the earliest failed transition.

## Invariants

1. Keep stock i2pd pinned at `635b013a612ff47278ef02acf8580a28e10e26c5` and
   `MAX_ATTEMPTS=1` for every runner invocation.
2. Keep signature type 7 explicit and retain PSK as a passing control.
3. Never persist raw credentials, secret-derived keys, raw reference logs, router
   addresses, or payloads. Evidence may contain only bounded booleans, enums,
   counts, and artifact hashes.
4. No production behavior change until a concrete i2pr-owned rejection is shown.
5. No transcript, dependency, support/conformance, or advertisement change.
6. Preserve all existing PSK/DH artifacts; no retry or replacement.

## Scope

In scope: audit credential generation, public-key publication, typed credential
parsing/sealing/opening, client activation, lookup scheduling, and resolver status
for DH; compare mode-specific inputs without writing secret material; add minimal
bounded diagnostic evidence or focused regression tests only when the current
surface cannot distinguish the transition; run all relevant local consumer guards
and one exact-pinned DH attribution attempt if controls are healthy.

Out of scope: DSA/type-0 support, new crypto providers, reference modifications,
Java lane, unrelated Plan 384 full floor, and broad proposal conformance.

## Work packages

1. Trace the runner's DH private/public derivation, emitted reference auth client
   record, and i2pr's typed `dh:<private>` credential through the sealed owner.
2. Identify the transition from `.b33` client creation through resolved target,
   listener binding, incoming request, remote lookup, and stream connect. Confirm
   which existing status/counter should advance at each stage.
3. Add only privacy-safe stage evidence needed to tell an early client-connect
   failure from delayed lookup or authentication rejection; negative-test its
   guard if adding static evidence.
4. Run relevant local Plan 380/381 credential tests and one exact-pinned DH lane
   only after all controls pass. Keep PSK as a separate passing control.
5. If an i2pr-owned defect is localized, register a corrective successor before
   implementing it. If the failure is external or unsupported, record that
   boundary and the required next plan.

## Failure and compatibility

Instrumentation must be bounded and transaction-scoped. A local test failure,
unhealthy mesh, or failed authority control invalidates the live attempt. No retry
within an invocation and no secret-bearing diagnostics.

## Verification

- Inspect exact-pin i2pd source for ELS2 DH record generation and consumer parse.
- Run focused `encrypted_service_consumer_wiring` and `i2pcontrol_els2_client_credential`
  tests if production credential handling changes or attribution needs regression
  coverage.
- ELS2 checker, runner self-test, evidence guard, and encrypted-consumer caller guard.
- At most one healthy exact-pinned DH attempt with `MAX_ATTEMPTS=1`.
- Full floor only after Plan 400's complete live matrix is green.

## Acceptance

The earliest failing DH transition is evidenced without secret leakage and its
owner is clear. Any i2pr implementation defect has a registered corrective before
the edit. Otherwise the limitation is classified precisely and Plan 400 remains
blocked. No unsupported DH compatibility or support claim is added.

## Closure evidence required

Record source symbols, the state transition and evidence, hashes for every run,
commands/outcomes, security/compatibility review, unresolved findings, and the
registry/roadmap unblock audit. Explicitly state that Plan 400 DH remains
unqualified unless the payload row passes.
