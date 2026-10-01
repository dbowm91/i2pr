# Plan 283 — M12 third-class evidence and controlled activation corrective

Status at registration:
**registered-m12-third-class-evidence-and-controlled-activation-ready**

Classification: narrow corrective / evidence strategy + daemon-runtime activation composition.

Predecessor evidence:
- Plan 277 stopped with partial coordinator/role work retained.
- Plan 282 stopped at the above-floor reachability evidence boundary
  (`plans/closure/floodfill/282-status.md`,
  `stopped-m12-activation-blocked-on-above-floor-reachability-evidence-corrective-via-plan283`).
  Retained: the runtime publication bridge, the atomic rotation API with policy and
  generation, the route-correct effect encoder, the hardened owner with outcome
  accounting and cancel-drain, the live direct-path suite (rows 3–9, 12), the
  persistence-restart proof (row 10), and the explicit-bind corroboration pair that
  yields exactly `CandidateReachable` on loopback-homogeneous traffic.
- Plans 278 and 279 remain blocked (278 now on this plan).

## 1. Objective

Close the exact gap Plan 282 stopped at: reach above-floor (`Reachable`) evidence
without fabricating publication state, then compose and prove the controlled
activation/withdrawal lifecycle that consumes it. On pass, this plan becomes the M12
local daemon-composition authority and unblocks Plan 278.

A successful Plan 283 proves, on the record, every remaining Plan 282 acceptance item:
criteria 2, 3, 4, 11 and §10 rows 2, 11, plus the live half of 13 — with no
weakening of the Plan 159 direct-publication contract, the type-5 deferral, or any
boundary guard.

## 2. The boundary being corrected

The Plan 159 publication builder renders a direct SSU2 address only for
`ReachabilityState::Reachable`, which requires three distinct corroboration classes.
Static loopback-homogeneous traffic factually yields at most two (explicit bind +
peer observation). Recording handshake Address blocks as observations was rejected
(no independent evidence), and accepting `CandidateReachable` in the builder was
rejected (would weaken Plan 159 from under its closure). The full analysis lives in
the Plan 282 stop record; this plan must not relitigate it.

## 3. Hard invariants

1. The Plan 159 direct-publication contract is not changed: `Direct` stays
   `Reachable`-only.
2. No corroboration class is ever recorded without the fact it names: binds must be
   explicitly configured and actually bound; observations must come from real
   authenticated traffic; any third class must come from its real protocol event.
3. `i2pr-daemon` must not depend directly on `i2pr-transport-ssu2`.
4. Normal daemon configuration still cannot construct the controlled floodfill permit.
5. `caps=f` remains unavailable outside the controlled qualification path.
6. Activation failure leaves the runtime on the previous non-`f` RouterInfo with the
   role non-Active (Plan 282 §12 failure semantics).
7. Type 5 remains deferred under Plan 281; no Red25519/provider work.
8. No wire-format change; persistence remains Plan 276 format v1.

## 4. Work package A — third-class evidence strategy (decision first)

Before writing activation code, the implementer must select exactly one strategy and
record the decision with its factual basis in the closure record:

- **Option 1 (preferred if true): heterogeneous reference traffic.** Demonstrate that
  controlled loopback traffic against exact-pinned i2pd 2.61.0 produces a genuine
  third class (peer Address-block observations, path-validation migration on
  rebind, or peer-test confirmation) through the *unmodified* Plan 278 lane
  scaffolding, without claiming any Plan 278 qualification row. If the third class
  materializes, activation proofs run in that lane; Plan 278 still owns its own
  qualification rows separately.
- **Option 2: loopback migration proof.** Demonstrate that a same-identity rebind
  (new socket, retained keys) migrates a live session through the real
  challenge/response path and records `ValidatedPath`, giving classes {bind,
  migration} — still only two. This option is documented here as INSUFFICIENT by
  itself (no Address blocks flow on i2pr↔i2pr loopback); it may only combine with a
  third factual class, never substitute for reasoning one into existence.
- **Option 3: registered peer-test driver.** If neither traffic source yields a
  third class, register the peer-test driver as its own interface dependency first:
  a bounded daemon-owned driver for the existing `transport-ssu2` peer-test state
  machine whose `Confirmed` outcomes feed `PeerTestResult`. This plan must not
  swallow that feature; size it separately if chosen.

Whichever option is selected, the closure record must show the corroboration classes
by name, the protocol events that produced them, and the tracker state transition to
`Reachable` — never a bare assertion that material "became available".

## 5. Work package B — controlled activation/withdrawal composition

In `crates/i2pr-daemon/src/floodfill.rs` (the only site the boundary guard permits):

- `activate_controlled`: explicit-bind recording → publication material →
  eligibility → `begin_activation` → `complete_activation` (permit) →
  `build_floodfill` with the material address → `install_local_router_info` →
  publish through the existing NetDB publication authority → done. Any failure
  after `begin_activation` fails the controller and returns the runtime to the
  previous non-`f` bytes.
- `withdraw_controlled`: failing eligibility snapshot → stop admission (Draining)
  → build the same-address non-`f` RouterInfo (add the narrow permit-gated
  withdrawal builder in `i2pr-netdb/src/local.rs` only if required) → install →
  publish → bounded drain to deadline → `complete_drain` → Disabled.
- Wire the retained `run_floodfill_owner` behind the activation (startup order per
  Plan 282 §9) with the existing outcome accounting and cancel-drain.

## 6. Work package C — close the remaining acceptance

- §10 row 2: controlled eligibility installs an RI containing the qualified SSU2
  address and `caps=f`; ordinary/default daemon remains unable to do so.
- §10 row 11: health loss removes `f`, stops admission, drains bounded effects,
  reaches Disabled — full sequence, not just the role transition.
- §10 row 13 (live half): future handshakes emit the latest installed RI while
  pre-existing sessions are untouched (byte-level proof, not just mechanism).
- Dial-permit baseline: cancel mid-dial returns dial admission to baseline
  (the one open half of row 12).
- Regression: every retained Plan 282 proof stays green unmodified in intent;
  changes to retained tests require explicit justification in the closure record.

## 7. Scope

In scope: the evidence-strategy decision and its implementation, the
activation/withdrawal composition, the remaining acceptance proofs, and
planning/support reconciliation.

Out of scope: exact-pinned i2pd qualification rows (Plan 278), Java-family work
(Plan 279), normal user floodfill config, public/non-loopback participation,
type-5/Red25519, new transports, M11 transit changes.

## 8. Static guards

Extend `scripts/check-m12-floodfill-boundaries.sh` at minimum:

- the activation/withdrawal composition lives only in daemon `floodfill.rs`;
- any withdrawal RI builder in `local.rs` requires the advertisement permit;
- no new corroboration recording site outside the runtime service owner;
- all Plan 282 guards stay green unweakened.

## 9. Required verification

Plan 282 §15 focused floor plus the new activation/withdrawal proofs, then the full
repository floor. Ordinary exact-head CI must be green before closure. If the full
floor stalls, retain the exact boundary and investigate; do not close from focused
tests only.

## 10. Documentation and support updates

On implementation/closure update: `plans/closure/floodfill/283-status.md`,
`plans/subsystems/floodfill-roadmap.md`, `plans/registry.md`, the touched
architecture deep-dives, `specs/support.toml`, and the README current-work line. Do
not state external floodfill interoperability until Plan 278 passes.

## 11. Acceptance criteria

Plan 283 closes only when the Plan 282 stop record's open items (criteria 2, 3, 4,
11; rows 2, 11, 13-live; dial-permit cancel baseline) pass with the evidence
strategy recorded, the full workspace floor and exact-head ordinary CI are green,
and no critical/high finding remains open.

## 12. Stop conditions

Stop and register a narrower corrective if the third class cannot be produced
without fabrication, if activation requires a daemon→`transport-ssu2` dependency, or
if rotation invalidates live session identity semantics. Do not compensate by
weakening corroboration, the Plan 159 contract, route checks, or budgets.

## 13. Closure and unblock audit

On pass: mark Plan 283 passed; retain Plans 277/282 as stopped/corrected-via-283;
move Plan 278 to ready; keep 279 blocked on 278, 280 stopped, type 5 deferred, and
normal/public `caps=f` unclaimed. On stop: record the boundary with the same rigor
as Plan 282 and register the next narrower corrective immediately.

## 14. Handoff

Plan 278 then exercises the completed controlled daemon path against unmodified
exact-pinned i2pd. No additional local architecture tranche should be inserted
unless Plan 283 closure evidence identifies a concrete remaining defect.
