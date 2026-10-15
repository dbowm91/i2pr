# Plan 484 — Bandwidth default burst vs NTCP2 wire-frame minimum

**Status:** passed 2026-10-15 — see `plans/closure/core-router-recovery/484-status.md`. **Owner:** daemon config default + NTCP2
runtime shaper (cross-owner policy, one coherent pass). **Baseline:** main
`7b54fdd6` (Plan 483 close). **Predecessors:**
`plans/closure/floodfill/479-status.md` (finding: 64 KiB default
`burst_bytes` below the NTCP2 wire-frame minimum),
`plans/closure/floodfill/454-status.md` (default 64 KiB burst rejected for
the dormant adapter; 128 KiB accepted),
`plans/closure/floodfill/483-status.md` (limitation carried forward).

## Objective (one bounded outcome)

Decide and land the single coherent `burst_bytes` default policy so a
bandwidth-enabled daemon constructs deterministically: either the default
burst fits a maximum NTCP2 data frame above the control reserve, or the
default stays 64 KiB and construction fails closed with an explicit error
naming both values. No silent default change, no lowered wire minimum.

## Why ready

- Both sides of the conflict are closed evidence, not open questions: the
  minimum (`ceil(65537·100/90) = 72819` at 10% reserve) is pinned by the
  Plan 454 regression
  `shared_shaper_must_fit_a_maximum_data_frame_above_the_control_reserve`;
  the default (64 KiB) is the committed daemon config default the 479 lane
  works around with explicit 128 KiB.
- Interface deps are stable: daemon `[bandwidth]` config owner and the
  NTCP2 runtime shaper adapter both exist with focused tests.
- No external topology, reference router, or live pair is required: the
  decision is provable with deterministic unit/config tests plus the
  existing dormant-adapter regression.

## Current implementation evidence

- `crates/i2pr-runtime`: dormant-NTCP2 adapter rejects default-sized 64 KiB
  burst, accepts 128 KiB (454-status verification row).
- Daemon comparator lanes (479/483) set 128 KiB explicitly with the
  derivation in a lane comment; defaults are never exercised with NTCP2.
- Any bandwidth-enabled daemon with NTCP2 currently fails construction at
  defaults (479 finding, uncontradicted).

## Invariants that must not regress

- The NTCP2 wire-frame minimum and control reserve are transport safety
  properties: the fix may not lower the minimum, shrink the reserve, or
  special-case the dormant adapter.
- A 64 KiB configured burst that cannot fit the frame must never
  silently clamp: explicit configured values keep exact semantics.
- Bandwidth stays default-off; no RouterInfo, support-inventory,
  advertisement, or public-posture change.
- No unbounded channels/queues; secret hygiene unchanged (no new secret
  surfaces expected; if the chosen design adds one, it follows the
  structural-redaction rule, not message filtering).

## Scope

In: exactly one of (a) raise the default burst to the smallest value that
fits a maximum data frame above the reserve, with the derivation pinned in
code and docs; or (b) keep the 64 KiB default and make construction fail
closed with an error naming the configured burst, the required minimum,
and the lane's explicit-override spelling. Deterministic regressions for
the chosen branch plus the rejected branch's guard (a test that fails if
the default silently drifts below the minimum again, or fails if an
explicit small burst is silently accepted). Docs update for the operator.

Explicitly out: per-transport burst tuning, reserve-ratio changes,
governor scheduling changes, any live-router or reference-router lane,
Plan 437/438/433/436 gates, ELS2 work.

## Required production changes

Confined to the daemon `[bandwidth]` default owner and/or the
construction-time validation that already rejects the dormant adapter —
no wire, codec, transport state-machine, or NetDB change.

## Ordered work packages

1. WP1 — Decision record: (a) vs (b) with the arithmetic, operator impact,
   and migration note; if the repo convention requires it, file the ADR
   first (accepted ADRs are superseded, never rewritten).
2. WP2 — Implement the chosen branch in the config/validation owner.
3. WP3 — Regressions: default-fits-minimum guard; explicit-small-burst
   semantics guard (fail-closed error, exact values named); re-run the
   existing Plan 454 dormant-adapter regression unchanged as the direct
   contract under test (not a lower-tier repeat: it is the gate this
   plan decides).
4. WP4 — Closure record, registry/roadmap update, unblock audit.

## WP1 decision record (2026-10-15): branch (a)

Raise the daemon default to the smallest burst that fits one maximum NTCP2
data frame above the default control reserve:

```text
MAX_WIRE_FRAME_LENGTH = 65_535 + 2 = 65_537
minimum = ceil(65_537 * 100 / (100 - 10)) = ceil(6_553_700 / 90) = 72_819
```

- Operator impact: none for any working daemon. `[bandwidth]` is
  default-off and experimental; no bandwidth-enabled daemon with NTCP2 can
  exist on the 64 KiB default today because construction fails, so no
  working configuration changes shape. Daemons that set `burst_bytes`
  explicitly keep exact semantics (small explicit values are still rejected
  by the unchanged runtime gate — no silent clamp is added).
- Migration: none on wire, peers, or persisted state. Raising
  `control_reserve_percent` above the default can push the required minimum
  above the new default; that path stays fail-closed at construction and is
  documented for the operator.
- No ADR: policy default for an experimental default-off knob, following the
  Plan 454 precedent (no ADR filed there either). The decision, arithmetic,
  and guards live in this plan record plus the code comment and operator
  doc.
- Branch (b) rejected because it adds a new error path and operator repair
  burden for a configuration that can never have worked; the fail-closed
  gate for explicit small values is preserved unchanged.

## Failure / cancellation / restart / contention semantics

Construction-time decision only: no runtime task, queue, retry, or
restart path is added. If (b), the error is emitted before any socket,
task, or NetDB owner starts, and a config reload path (if any) applies
the same predicate.

## Compatibility and migration

If (a): daemons that relied on the 64 KiB default shape traffic
differently after upgrade — document the new default value and the
explicit-override spelling. If (b): daemons enabling `[bandwidth]` +
NTCP2 on defaults get a new fail-closed error where construction
previously failed opaquely — document the error and the override. No
wire, peer-visible, or persisted-state migration in either branch.

## Required tests

- Unit: default burst vs wire-frame minimum relationship (exact
  inequality, both directions guarded).
- Unit: explicit sub-minimum burst → typed fail-closed error naming both
  values (branch b), or exact preserved semantics (branch a, if small
  explicit values stay legal).
- Existing: Plan 454 dormant-adapter regression green unchanged.
- Inversion: each new test fails under a one-line mutant (lowered
  minimum, silent clamp, dropped error), mutants reverted.

## Exact verification commands

```text
cargo test --locked -p i2pr-runtime --lib ntcp2_runtime -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib bandwidth -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib config -- --test-threads=1
cargo fmt --all --check
cargo clippy --locked -p i2pr-runtime -p i2pr-daemon --all-targets -- -D warnings
python3 scripts/check-global-plan-number-uniqueness.py
```

Full `AGENTS.md` floor at closure.

## Documentation updates

- Operator-facing bandwidth config docs: default value with derivation
  or the fail-closed error with the override spelling.
- Registry row 484 ready → closed; core-router-recovery roadmap §7 row.

## Acceptance criteria

The chosen branch is implemented, both guards pass with inversion
probes, the 454 baseline is green unchanged, floor is green except
recorded-external reds, and no support/capability/advertisement change.

## Stop conditions

If the decision requires changing the wire-frame minimum, the reserve
ratio, or per-transport tuning, stop: that scope belongs to a transport
plan-of-record, not this policy pass. If a live-router lane proves
necessary, stop and re-scope: this plan is decision-provable offline.

## Closure evidence required

Implementation commit; requirement-to-evidence matrix (decision,
implementation, both guards with mutants, 454 baseline); commands with
outcomes; invariant/security/migration review; limitations; roadmap
disposition; unblock audit (433/435/436/437/438 unaffected expected).

## Handoff notes

- Consumes: 479 finding, 454 regression, 483 limitation — all linked
  above; nothing is re-derived.
- Does not unblock 437/438 (bandwidth floor is necessary, not
  sufficient, for the normal role) and does not touch 478/480 live legs.
