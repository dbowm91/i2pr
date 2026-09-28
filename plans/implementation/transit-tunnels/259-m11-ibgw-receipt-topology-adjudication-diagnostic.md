# Plan 259 — M11 IBGW receipt-topology adjudication diagnostic

Status at registration:
**registered-m11-ibgw-receipt-topology-adjudication-diagnostic-ready**

Baseline: `38c939a`

Corrects:
- plans/implementation/transit-tunnels/258-m11-ibgw-data-plane-multicell-diagnostic-corrective.md
- plans/closure/transit-tunnels/258-status.md

Retains:
- Plans 250/252/253/254/256 semantics and infrastructure.
- The complete Plan 257 reply/state/evidence implementation.
- The complete Plan 258 telemetry + H3 emission correction
  (fragment-for-all-sizes path, shared-threshold classifier,
  drop-side dimensions, per-ingress labels, checker guards).

Reference authority remains exact-pinned, unmodified i2pd 2.61.0 at
`635b013a612ff47278ef02acf8580a28e10e26c5`.

## 1. Objective

Adjudicate the single open boundary from Plan 258: after the H3
correction, datagram-sized ingresses to accepted IBGW
registrations emit 2 cells with zero forward failures and the
reference endpoint reassembles the garlics, but no payload ever
reaches the receiver socket (`gateway-receipt: 0` in all four Plan
258 executions). Determine, with cited evidence, whether receipt
is achievable in the controlled lane at all — and if not, record
exactly which premise fails so the lane can close honestly.

This is a **diagnostic adjudication plan**. It is not permission
to weaken the receipt row, to deviate from reference wire
behavior (Local delivery is what the reference IBGW emits —
source-verified), to force lane topologies to go green, or to
claim transit capability.

## 2. Why Plan 259 is required

Plan 258 closed retained with:

- H2 excluded (zero forward failures everywhere).
- H3 found, fixed, and externally proven (`multicell-bounded:
  true`, 3× 2-cell emissions, B-side reassembly confirmed in the
  reference log at the exact emission seconds).
- Our emission byte-proven correct (in-tree reference-endpoint
  parse simulation: layer inversion + delimiter sync + record
  parse + reassembly == nested bytes, first flag `0x08`).
- A residual receipt gap with cross-run uniformity: EVERY
  fragmented garlic at B endpoints — including garlics that never
  transited us — is reassembled but never routed onward here.

The structural hypothesis (to be confirmed or refuted, never
assumed): our IBGW registration's next hop is B, B treats
arrivals as its tunnel endpoint, and the receiver destination
lives on A. A transit endpoint can only complete receipt on its
own router; foreign-destined Local garlics are never routed
onward in this topology. If true, the length-2 `[i2pr, B]`
receiver tunnel can never close receipt, and the row's premise
(not the code) is the finding.

## 3. Frozen invariants

Same as Plan 258 §3 (exact pin, unmodified reference,
loopback-only, fresh datadirs, no gate redefinition, no retry
tuning, sanitized counts only), plus:

- No delivery-type deviation: IBGW injections stay Local (the
  reference hardcodes it; the checker already forbids the
  `build_single` branch — extend the guard to any non-Local
  injection if touched).
- No lane forcing: explicitPeers/lengths/session shapes stay as
  the lane defines them; observing what topologies the reference
  builds is research, rearranging them for row color is tuning.
- No reference patching, no public network, no new dependencies.

## 4. Work packages

### A. B-endpoint dispatch inventory (reading only)

Against the pinned source (already vendored under
`target/interop/ssu2-sources/`, never patched), inventory every
post-reassembly path for a foreign-destined Local garlic at a
transit endpoint: Local arm → `HandleI2NPMessage` → garlic arm →
tag-miss behavior (sync logs + async `PostGarlicMessage` lines,
with thread ids). Cite the exact lines that explain the observed
silence (or the forward arm if one fires). Output: a source-locked
note naming the dispatch outcome — no code change.

### B. Lane-topology survey (evidence only, no new runs required)

Across the retained Plan 257/258 evidence (`target/interop/
m11-plan257-*`, `m11-plan258-*` ledger TSVs): tabulate every
accepted IBGW registration's `(receive_id, next_router)` pairs
per run, and whether any registration ever chains toward A
(next_router == A peer). If A-ending topologies never occur,
that is the finding. No new external execution is authorized by
this package alone.

### C. Adjudication (one fork, decided by A+B evidence)

- Fork 1 (a closing topology exists in-lane): exhibit it with
  ledger citations and replan WP D (two counted same-SHA passes)
  as a narrow follow-up. This plan does NOT execute the passes.
- Fork 2 (no closing topology exists): record the receipt-row
  premise revision — receipt belongs to the OBEP direction
  (which passes with 512 B + 4,096 B payload-verified receipts);
  the IBGW lane's terminal proven rows are ingress +
  multicell-bounded emission + zero-failure forward. The closure
  states what M11 may then claim (experimental one-family
  transit emission, still non-advertised) and what stays open.
  No claim beyond the evidenced subset.

### D. Constant fragment message-id hardening (if C keeps emission work open)

Only if Fork 1: replace the constant `message_id: 1` with a
per-registration monotonic fragment id (or the nested I2NP
msgid via a new `i2pr-proto` accessor if that stays narrow),
with a regression proving distinct ids across sequential
fragmented emissions. If Fork 2, record it as deferred hardening
instead — do not touch the proven path for robustness alone.

## 5. Required focused tests

At minimum (new tests only where behavior changes):

1. Existing Plan 258 emission/telemetry/checker rows stay green
   (no suppression).
2. If D executes: distinct fragment ids across sequential
   fragmented emissions; parser still accepts; checker pins the
   counter (no constant id).
3. Adjudication cites: ledger row counts per topology bucket;
   source-lock line numbers for the dispatch finding.

## 6. Exact verification floor

Plan 257 §19 floor verbatim (fmt, check, workspace tests, clippy
`-D warnings`, doc, doc-tests, deny, all boundary and acceptance
scripts, `git diff --check`), plus every new Plan 259 focused
test by name. No external execution is required to close unless
Fork 1 replans WP D (then the two-pass gate applies there, not
here). Exact-head ordinary CI green on all four required jobs is
required at closure (this also covers the Plan 258 line, whose
hosted CI is still unobserved).

## 7. Acceptance criteria

Plan 259 closes only when all are directly evidenced:

1. The B-endpoint dispatch inventory names the exact silent path
   with source-lock citations.
2. The topology survey tabulates `(receive_id, next_router)`
   across retained runs with counts.
3. Exactly one fork is taken with cited evidence; the other is
   recorded as refuted (or out of scope with reason).
4. Fork 1: WP D replan registered (passes not executed here).
   Fork 2: receipt-row premise revision recorded; M11 claim
   restated within the evidenced subset; no capability beyond it.
5. Full workspace verification passes; exact-head
   Ubuntu/macOS/MSRV/dependency-policy all pass.
6. No known critical/high finding remains open without a named
   owner (the Plan 258 medium msgid finding is either fixed via
   D or re-owned here).

Only after all 6 may the unblock audit mark
`m11_transit_qualification` (Fork 1 needs its WP D passes first;
Fork 2 needs the restated claim) and register M12 floodfill
planning.

## 8. Stop conditions

Stop and record the exact boundary rather than broadening the
plan if:

- the dispatch inventory is inconclusive from logs + source
  (then the finding is "unobservable at this instrumentation",
  still stop);
- exhibiting a closing topology would require reference
  patches, public network, or lane forcing;
- a new dependency or public transit toggle is proposed.

## 9. Documentation and closure evidence

On closure, write plans/closure/transit-tunnels/259-status.md
with the Plan 257 §22 items plus: the dispatch inventory with
source locks, the topology survey table, the fork decision with
citations, and (Fork 2) the restated M11 claim.

Update: plans/registry.md,
plans/subsystems/transit-tunnels-roadmap.md, specs/support.toml,
specs/protocols/05-tunnels.md, specs/CONFORMANCE.md only if
support state actually changes.
