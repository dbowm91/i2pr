# Plan 258 — M11 IBGW data-plane multicell diagnostic corrective

Status at registration:
**registered-m11-ibgw-data-plane-multicell-diagnostic-corrective-ready**

Baseline: 1784ff87a8939c038ae6d8d2d574b73332be9cd0

Corrects:
- plans/implementation/transit-tunnels/257-m11-production-self-reply-and-external-evidence-completion-corrective.md
- plans/closure/transit-tunnels/257-status.md

Retains:
- Plan 250 admission/reply/registry semantics.
- Plan 252 runtime-neutral full-message ShortTunnelBuild transaction.
- Plan 253 runtime-neutral data plane, rollback/drain, and bounded peer-state work.
- Plan 254 live ingress/body-threading owner boundary.
- Plan 256 identity/key-coherence, exact NetDB bootstrap, real i2pd-B topology,
  typed role evidence, OBEP/IBGW data-plane work, and anti-fan-out infrastructure.
- The complete Plan 257 implementation: SelfReplyOtbrmArgs clippy-ceiling fix,
  TransitLiveStateSnapshot seam, typed TransitBandwidthSummary plumbing,
  i2pd reply-branch and endpoint source locks, exact cardinality/bandwidth/
  far-side/full-drain/session-close/real-restart driver evidence, two-attempt
  workflow gate, and the Plan 257 checker/boundary hardening.

Reference authority remains exact-pinned, unmodified i2pd 2.61.0 at
635b013a612ff47278ef02acf8580a28e10e26c5.

## 1. Objective

Diagnose and narrowly correct the retained IBGW data-plane boundary that
stopped Plan 257, then complete the Plan 257 external evidence on the
corrected lane.

Plan 257 implementation head 1784ff8 executed the complete corrected
exact-pinned i2pd matrix three times on one SHA (two counted attempts plus
one B-loglevel diagnostic). All three stopped at the identical retained row
with the identical signature:

- `ibgw-data/gateway-delivered-count` = 9 / 13 / 14 genuine single-cell
  deliveries to the accepted IBGW registration;
- `ibgw-data/multicell-max` = 1 (no 2+ cell emission observed);
- `ibgw-data/gateway-receipt` = 0 (no payload-verified 1500-byte receipt);
- i2pr SSU2 sessions to both references stable throughout
  (`sess=2+0/0`, `nosess=0`, no queue drops, no auth failures);
- the B-loglevel diagnostic (B at info instead of debug) reproduces the
  identical signature, exonerating Plan 257 work package F's B-debug
  requirement as a destabilizing factor.

Plan 258 has one bounded outcome: determine whether the missing multicell
emission is an emission-side fact (reference never delivers a complete
multi-cell batch to the IBGW registration), a delivery-side fact (cells
emitted but onward delivery fails), or a production data-plane defect
(missing reassembly/fragmentation on the IBGW ingress path), land the
narrow in-scope correction with regression telemetry, and then execute the
complete corrected matrix twice on one SHA with every mandatory row green.

This is a **diagnostic corrective plan**. It is not permission to weaken
the multicell row, to tune retries until green, to add public transit, or
to broaden protocol claims.

## 2. Why Plan 258 is required

### 2.1 Plan 257 crossed no stop condition of its own, but the retained lane did

Plan 257's reply/state/evidence implementation is locally proven (261 lib
rows including 10 new Plan 257 service regressions, 40 driver-local rows
including 12 new Plan 257 predicate rows, full workspace floor green,
both static checkers green). The external lane stops before any Plan 257
evidence row executes: the driver aborts fail-closed at the retained
`multicell-bounded` gate, so cardinality, typed bandwidth, far-side,
full-drain, session-close, and real-restart evidence never executes
externally. Those rows are implemented but externally unproven.

### 2.2 The failure signature is systematic, not flake

Three same-SHA executions (attempts 1, 2, plus the info-B diagnostic) fail
at the same driver line
(`m11_transit_i2pd_external.rs` multicell gate) with the same quantitative
signature (single-cell-only ingress batches, zero receipt, healthy
sessions). Per Plan 257 §21 this localizes a boundary outside the Plan 257
reply/state/evidence scope and must be preserved, not retried until green.

### 2.3 The lane cannot currently distinguish the three hypotheses

- H1 (emission-side): the reference never delivers a complete >1-cell
  nested batch to the accepted IBGW registration in this topology
  (upstream fragmentation, alternate routing, stale sender pools).
- H2 (delivery-side): i2pr emits 2+ cells but onward delivery fails
  (`NoActiveSession`/resource) — the lane records `delivered` but drops
  the `failures` count, so this is currently unobservable.
- H3 (production): i2pr's IBGW ingress path never reassembles multi-cell
  tunnel messages before gateway dispatch, so no stimulus can ever emit
  multi-cell — a genuine data-plane defect.

The lane records neither per-ingress nested sizes nor forward-failure
counts. Plan 258 must add that telemetry first, then follow the data.

## 3. Frozen invariants

Same as Plan 257 §3 (exact pin, unmodified reference, loopback-only, no
public reseed/network, fresh datadirs per attempt, RouterIdentity/build-key
coherence, SSU2 key separation, single-owner/single-decode ownership,
bounded state, no per-cell tasks, no version/capability/advertisement
change, no public transit, no M12 until two-pass closure). In addition:

- The `multicell-bounded` row keeps its current predicate (2+ Accepted
  cells from one ingress bound to an accepted registration). No
  redefinition, no single-cell acceptance, no background-traffic counting.
- No retry-count, timeout, round-count, or redial tuning whose only
  justification is making the row pass. Redial/session changes require
  independent session-death evidence (snapshot `sessions_closed` delta or
  `nosess`/`send_without_link` counts), not row color.
- Diagnostic telemetry is sanitized counts/sizes only; no payload bytes,
  no keys, no raw logs in evidence.

## 4. Work packages

### A. Failure telemetry (observability only, no gate change)

1. Surface the gateway forward `failures` count into the typed ledger
   observation for `GatewayDelivered` (alongside `delivered`), plus the
   nested message size class (single-cell vs multi-cell-capable) derived
   from already-decoded lengths.
2. Record per-ingress sanitized rows (diagnostic-only keys, never counted
   toward the multicell gate): ingress count, nested size distribution,
   emitted-cell distribution, failure distribution by outcome kind.
3. Extend the static checker so a `GatewayDelivered` observation without
   the failure dimension fails structurally.

### B. Hypothesis test (one diagnostic execution, non-counted)

Run the instrumented lane once (fresh datadirs, same SHA, explicitly
labeled diagnostic, never merged into counted evidence) and classify:

- failures > 0 with multi-cell-capable nested batches → H2.
- failures == 0 with only single-cell nested batches → H1 or H3.
- nested multi-cell-capable batches emitted single-cell → H3.

### C. Narrow correction by hypothesis

- H2: repair onward session liveness narrowly (redial only on proven
  session death, never blind; regression test with injected close).
- H1: repair the stimulus/topology narrowly (sender/receiver pool shapes
  that deterministically route the complete batch through the accepted
  IBGW registration; prove with nested-size telemetry, not row color).
- H3: repair the ingress reassembly/fragmentation path narrowly in
  `i2pr-tunnel` and/or the daemon gateway seam with focused
  runtime-neutral regressions first.
- If the defect requires redesign beyond one ownership boundary, new
  dependencies, public behavior, or reference patching → stop and record
  the boundary instead. Do not proceed.

### D. Complete the Plan 257 external evidence

After the narrow correction, execute the complete corrected matrix twice
on one SHA with fresh datadirs. Both attempts must pass every mandatory
row, including the previously unexecuted Plan 257 rows (cardinality,
typed bandwidth, far-side with B endpoint binding, full-drain cancel with
new-ingress-refused, A-removed/B-retained session close, real runtime
restart with fresh-build-accepted). Then require exact-head ordinary CI
green (Ubuntu Quality, macOS Quality, MSRV, dependency policy).

## 5. Required focused tests

At minimum:

1. gateway observation carries the failure dimension (structural);
2. nested size classes distinguish single-cell from multi-cell-capable;
3. H2 regression (if taken): proven-dead session redialed, live session
   never redialed;
4. H1 regression (if taken): complete batch routes through the accepted
   registration deterministically in fixtures;
5. H3 regression (if taken): multi-cell ingress reassembles then emits
   multi-cell in runtime-neutral tests;
6. multicell gate still rejects single-cell-only batches;
7. background traffic to unaccepted ids still cannot satisfy rows;
8. two-pass same-SHA gate unchanged (reuse Plan 257 predicates).

## 6. Exact verification floor

Plan 257 §19 floor verbatim (fmt, check, tunnel tests, daemon tests,
workspace tests, clippy `-D warnings`, doc, doc-tests, deny, all boundary
and acceptance scripts, `git diff --check`), plus every new Plan 258
focused test by name, plus two complete same-SHA external passes, plus
exact-head ordinary CI green on all four required jobs.

## 7. Acceptance criteria

Plan 258 closes only when all are directly evidenced:

1. Failure telemetry lands and the checker enforces it.
2. One non-counted diagnostic execution classifies H1/H2/H3 with
   sanitized numbers cited.
3. The narrow correction lands with focused regressions; no gate
   redefinition, no retry tuning to go green, no public behavior change.
4. The retained Plan 257 implementation remains intact (all local rows
   green; no suppression added).
5. Complete external attempt 1 passes every mandatory row.
6. Complete external attempt 2 passes every mandatory row on the same SHA
   with fresh datadirs.
7. Full workspace verification passes; exact-head Ubuntu/macOS/MSRV/
   dependency-policy all pass.
8. No known critical/high finding remains.

Only after all 8 may the unblock audit mark
`m11_transit_qualification = passed-via-i2pd-2.61.0` for the ADR 0026
experimental gate and register M12 floodfill planning.

## 8. Stop conditions

Stop and record the exact boundary rather than broadening the plan if:

- the defect requires redesign beyond one ownership boundary;
- deterministic completion requires patching i2pd or public network;
- the multicell row is unachievable in the controlled topology for
  reference-behavior reasons (then the row's premise, not the code, is
  the finding — still stop, never weaken);
- a new dependency or public transit toggle is proposed.

## 9. Documentation and closure evidence

On closure, write plans/closure/transit-tunnels/258-status.md with the
Plan 257 §22 items plus: the H1/H2/H3 classification numbers, the narrow
correction diff with regressions, both same-SHA attempt manifests, and
the disposition of every Plan 257 retained row.

Update: plans/registry.md, plans/subsystems/transit-tunnels-roadmap.md,
specs/support.toml, specs/protocols/05-tunnels.md, specs/CONFORMANCE.md
only if support state actually changes.
