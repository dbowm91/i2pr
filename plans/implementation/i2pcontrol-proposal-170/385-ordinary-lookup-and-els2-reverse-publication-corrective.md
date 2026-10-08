# Plan 385 — ordinary lookup and ELS2 reverse publication corrective

Status: **in-progress-ordinary-lookup-and-reverse-publication-corrective**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Corrects Plan 384, whose authoritative blocked record is
`plans/closure/i2pcontrol-proposal-170/384-status.md`. Plan 384's local
post-start authority provisioning and gossip gate execute, but the ordinary
client does not return the reference publisher's payload after a fully
converged mesh. The reverse i2pr-publishes direction was consequently not
attempted. Plan 384's evidence and source changes remain authoritative; this
plan must not rewrite its result.

## Objective

Close the pinned i2pd 2.61.0 direction in both directions: establish and fix
the ordinary standard-LeaseSet lookup failure seen by Plan 384, then prove
post-start authority payload and i2pr-published ELS2 payload for NONE, PSK,
and DH. The result is a live interoperability result only for the exact rows
executed; it does not promote Proposal 170 support or advertisement.

## Why ready

Hard/interface dependencies are closed: Plans 346, 350, 351, 380, and 381.
Plan 384 is a blocked evidence input, not a passed capability dependency; its
closure record supplies the failure reproduction, runner, sanitized evidence
schema, gossip-convergence gate,
manager generation checks, and a post-start authority driver row. No new
protocol decision is required. The open question is an implementation and
evidence gap in ordinary lookup/publication; any transcript-profile or
inventory change is explicitly out of scope and triggers a stop.

## Current evidence and unresolved findings

- Plan 384's `plan334_els2_create_get_rawconfig_round_trip_over_jsonrpc`
  pins the local encrypted-address `get` projection.
- Its post-start authority row records successful create, manager generation
  advance, committed target projection, and listener bind.
- With all three controlled i2pd floodfill candidates logging the reference
  standard LeaseSet before the request, the ordinary payload still failed.
  A temporary five-attempt lookup experiment failed too and was reverted;
  ordinary lookup remains at its existing three-attempt budget.
- The i2pr-publishes → i2pd-consumes NONE/PSK/DH matrix has no passing rows.
- During this corrective, the standard post-start authority payload passed after
  fixing the delayed ordinary-client activation and validated-mirror resolution
  gates. Reverse NONE and PSK attempts then reached i2pd but returned
  `CANT_REACH_PEER` / `LeaseSet not found`. A later valid DH run reached the
  same reverse lookup and returned the same result. The complete reverse matrix
  is therefore still open and is assigned to Plan 386.
- Plan 384's gossip-gate and standard-lookup observations are captured in its
  sanitized evidence. Invalid runs, including the reference mesh failure,
  are not passing evidence.

## Invariants

1. Use stock i2pd `2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`;
   do not patch, vendor, or rebuild a modified reference.
2. Preserve `MAX_ATTEMPTS=1` in the external lane. Do not raise the ordinary
   lookup retry budget to mask an interop defect. Any production attempt
   policy change requires a separately reasoned bounded design and regression
   evidence, and may not violate this lane invariant.
3. Keep the deployed type-11 transcript profile (ADR 0032), Proposal 170's
   frozen field inventory, and Plan 380's typed credential seam unchanged.
4. Keep type 5 non-advertised; do not set `full-proposal-conformant` or add
   `support.toml` claims.
5. Missing reference inputs fail closed. No fake peer, broad exclusion,
   `|| true`, or continuation after a failed gate.
6. Evidence contains sanitized counts/hashes only; never commit raw reference
   logs or credential values.
7. Every payload row traverses the real transport and pinned reference. Do
   not inject decoded LeaseSets or call private resolver/bridge/pump APIs from
   the black-box driver.

## Scope

### In scope

- Trace the ordinary standard `DatabaseLookup` path from the committed
  post-start client target through peer selection, request/reply handling,
  lookup attempt accounting, and Stream delivery. Add bounded diagnostic
  evidence that distinguishes a miss, timeout, invalid reply, and unresolved
  destination without exposing payload or secrets.
- Reproduce against the existing gossip gate and exact reference pin; fix the
  demonstrated product defect at its owner and add focused regression rows.
- Prove the ordinary post-start authority payload row after the lookup fix.
- Execute reverse encrypted-server payload rows for NONE, PSK, and DH, plus
  the existing mesh control and consumer-direction control.
- Extend Plan 384's checker and packaged sanitized evidence with each
  replacement row. Keep an absence guard for any row that remains unproven.

### Explicitly out of scope

- Changing the wire transcript, signature profile, Proposal 170 inventory,
  or authorized-consumer credential seam.
- Increasing the external runner's attempt budget, weakening the gossip gate,
  modifying reference software, or claiming the full Proposal 170.
- Java I2P qualification (Plan 375), cross-family convergence (Plan 377),
  or terminal conformance (Plan 378).

## Ordered work packages

1. **Diagnose:** inspect the sanitized Plan 384 rows and ordinary lookup owner;
   add the minimum local instrumentation/regression fixture necessary to
   identify the exact failed transition. Do not guess from elapsed timeout.
2. **Correct:** fix that owner under the existing bounded budgets. Add tests
   for the observed defect and its failure/cancellation paths. If the evidence
   instead points to the transcript or frozen seam, stop and register a new
   plan rather than changing those authorities here.
3. **Requalify authority:** run the exact-pinned lane with all floodfills
   passing the existing gossip gate; require the post-start standard payload
   row and all existing local generation/projection checks.
4. **Reverse matrix:** run NONE, PSK, and DH separately with fresh
   credentials, sanitized outputs, `MAX_ATTEMPTS=1`, mesh authority control,
   and the i2pd-consumes-i2pr payload row. Do not report a partial matrix as
   complete.
5. **Close:** update the checker, evidence JSON/Markdown, implementation
   closure record, roadmap, and registry. Run the downstream unblock audit.

## Lifecycle and compatibility

Lookup requests, timers, and temporary state remain owner-bounded and are
cancelled with the service lifecycle. A lookup failure must leave the service
in a typed failed/unresolved state and must not retry blindly after cancellation
or restart. No persistent format or public control schema changes are expected.
If the diagnosis requires a schema or operator migration, stop and register
that separately.

## Verification

Run focused local rows first, after building the managed-app sibling binaries
as required before any focused `i2pr-daemon` test:

```text
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_black_box -- --test-threads=1
bash tests/integration/els2/run-i2pd-els2.sh --self-test
python3 scripts/check-els2-live-lane-evidence.py --self-test
python3 scripts/check-els2-live-lane-evidence.py --mutation-table
bash scripts/check-els2-live-lane-evidence.sh
bash scripts/check-encrypted-service-consumer-caller.sh
```

Then execute the exact-pinned NONE/PSK/DH lanes through
`tests/integration/els2/run-i2pd-els2.sh`, preserving each run's sanitized
evidence separately. Finish with the AGENTS.md routine floor and the relevant
ELS2 evidence guards. Record unavailable host/toolchain checks truthfully;
never turn them into a pass.

## Acceptance and stop conditions

Pass requires the authority payload and reverse NONE/PSK/DH payload rows,
mesh control, consumer control, all local Plan 384 checks, checker mutation
controls, and the routine floor to pass. The checker must reject missing rows,
pin drift, attempt-budget drift, unsanitized evidence, and a falsely green
failed lane.

Stop and register a new corrective if: the gossip gate cannot establish the
reference LS2; the failure requires transcript/inventory/seam changes; a
partial auth matrix is the only achievable result; or the row requires
weakening bounded lifecycle, secret, or advertisement invariants.

## Closure record requirements

Record commit IDs; a requirement-to-evidence matrix; exact commands and
outcomes; which auth modes actually ran; sanitized evidence hashes; invariant,
failure, cancellation, restart, compatibility, and secret-handling reviews;
findings by severity; and the Plan 374/375/377/378 unblock audit. Do not claim
Plan 374 delivered until its remaining i2pd rows pass.
