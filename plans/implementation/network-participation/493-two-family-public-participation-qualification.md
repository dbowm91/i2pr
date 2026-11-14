# Plan 493 — Two-family public participation qualification

**Status:** registered, **blocked** on the authorized two-host topology
(Plan 431/472/476/480) and the transport-activation evidence (Plan 435) —
phase 6 of
[network participation](../../subsystems/network-participation-roadmap.md))
**Owner:** the evidence lane and its checkers; the participant roles
themselves are owned by Plans 488–492
**Baseline:** `work/plans-477-480-router-recovery@c43f3404`
**Authoritative predecessor:** `plans/closure/core-router-recovery/487-status.md`
(the closed client-product and §6 recovery line), Plan 468's lane
construction work, and the mixed-router-interop roadmap
**Research input:** `specs/CONFORMANCE.md` item 7 (the two-family rule) and
the pinned i2pd 2.61.0 and Java I2P 2.13.0 reference behaviors
**Predecessor lineage:** 431 (topology) → 435 (transport activation) →
488 → 489 → 490 → 491 → 492 → **493**

**Gate binding (Plan 495, 2026-10-24).** This plan's matrix runs under the
peer-eligibility gate whose canonical model is
[`specs/QUALIFICATION-GATES.md`](../../../specs/QUALIFICATION-GATES.md) and
whose operator procedure is
[`docs/operations/ssu2-private-lan-qualification.md`](../../../docs/operations/ssu2-private-lan-qualification.md),
enforced by Plan 495 (`plans/implementation/core-router-recovery/495-peer-eligibility-preflight-and-first-failure-owner-gate.md`)
through `scripts/plan495-peer-eligibility.py`.
Every leg below presumes the stock peer will select and dial `i2pr`; the gate
makes that presumption an explicit, checked precondition and makes the
lane's outcome say **which side owns a failure**. A family that admits our
RouterInfo but declines to select us is `peer-selection-refused` — a result
about that family's selection state, not an `i2pr` defect, and never a pass.
The attempt-budget rule in invariant 6 is reconciled with the gate below.

## Objective

Produce the **two-family interoperability evidence** for `i2pr`'s
participant roles — the DHT spine, the SSU2 introducer role, and transit
participation — against **two independent router families** (i2pd 2.61.0
*and* Java I2P 2.13.0), in an environment where the routers are actually
addressable by one another.

One bounded outcome: **a two-family evidence matrix over the participant
surface, with sanitized counts and hashes, and no capability claim beyond
it.**

### Why this plan is registered blocked

The qualification needs two things the repository does not have:

1. **an independently addressed, non-loopback topology** — Plan 431 stopped
   at exactly this boundary, and its own record says it resumes only with an
   authorized such topology (Plan 472/476/480 own the runner; Plan 449 owns
   the portable LAN lane);
2. **transport activation evidence** — Plan 435, which is itself blocked on
   the baseline and on interoperability evidence that this plan's outcome
   feeds.

Registering it as blocked is honest and is the point of the unblock audit:
no plan silently waits for a topology that nobody has authorized.

## Why ready (when the topology opens)

- **Hard dependencies:** Plans 488–492 closed (the participant surface
  exists and is evidenced in a controlled mesh), plus Plan 431's topology
  and Plan 435's transport activation.
- **Interface dependencies:** the lane's command-line surface (the
  `run-*.sh` + `check-*-evidence.sh` pattern the external lanes already
  use) and the participant roles' receipts.

## Current implementation evidence

Present today:

- `specs/CONFORMANCE.md` item 7 — "mixed-router interoperability against
  at least two independent implementations for router-to-router protocols",
  with Java I2P or I2P+ plus i2pd as the preferred pair, and the explicit
  note that Java I2P and I2P+ share lineage and count as **one** family.
- The external-lane pattern: `bash tests/integration/<area>/run-*.sh` plus
  `bash scripts/check-*-evidence.sh`, with the environment-gated
  `#[ignore]` rule and the "missing environment must fail" rule.
- The reference pins: i2pd `635b013a612ff47278ef02acf8580a28e10e26c5`, Java
  I2P `9134f808337b401e8e53c73734c81fab04280c9d`, i2psam
  `b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`, i2plib
  `6edf51cd5d21cc745aa7e23cb98c582144884fa8`.
- The closed lanes that prove the *machinery* (the SSU2 lane, the M6 mixed
  router lane, the §6 lifecycle lane) — all of them single-family or
  loopback.

Missing today:

- **no two-family evidence for the participant surface** — the mixed
  router interop roadmap's Java full-router compatibility is retained and
  deferred (Plan 247), and `m6_full_two_family_router_conformance` is
  `not-yet-claimed` in `specs/support.toml`;
- **no public-network lane** — every lane today is loopback or private.

## Invariants that must not regress

1. **Two independent families, or no claim.** Java I2P and i2pd count as two
   families; Java I2P and I2P+ count as one.
2. **Fail-closed lanes.** A missing environment, a missing reference
   checkout, or a missing pin **fails** the lane; it never silently skips.
3. **No boundary script is weakened.** No `|| true`, no
   `continue-on-error`, no filename filtering, no fake peer environment, no
   broad exclusions, no early-return-success, and no production wire change
   made to go green.
4. **Pins are frozen.** The four reference pins above do not move without
   a new plan-of-record; a reference is never patched or vendored.
5. **Raw reference logs are never evidence.** Only sanitized counts and
   hashes reach the evidence file.
6. **Bounded execution.** A lane run must be licensed by a recorded
   material change; an unchanged-topology retry is not a new attempt.
   **Reconciled by Plan 495:** a *peer-state* change is a material change.
   A peer's ban expiry, a reset peer profile store, or a corrected peer
   configuration licenses a controlled rerun inside this plan, exactly as the
   canonical bounded-execution policy requires ("an *identified cause*
   (including reference fixture/env mistakes…) permits a correction… and
   rerun **inside the still-active plan**… no rule fixes a global attempt
   count of one"). A run classified `peer-state-poisoned` does not consume
   the attempt budget. A run classified `peer-selection-refused` records the
   family's own exclusion reason so the next attempt can fix it rather than
   re-run blind.
7. **No advertisement beyond the evidence.** Whatever this plan proves, the
   support inventory states exactly that and no more.

## Scope

**In:**

- the two-family lane for the **participant surface**: exploration and
  lookup (Plan 489), the SSU2 introducer role (Plan 490), and transit
  participation (Plan 492) — each exercised against i2pd 2.61.0 and Java
  I2P 2.13.0;
- the **evidence matrix**: per capability × per family, the outcome and
  the sanitized counts;
- the **checker** that validates the evidence file's shape and the
  matrix's completeness;
- the support-inventory and conformance updates that the evidence earns.

**Explicitly out:**

- **the floodfill role** and `caps=f` (Plan 494) — it needs this plan's
  evidence first;
- **any change to the reference routers**;
- **new protocol capability** — this plan qualifies what exists;
- **anonymity or privacy claims** of any kind;
- changing the reference pins, the boundary scripts, or the lane's
  fail-closed semantics.

## Required production changes

- `tests/integration/network-participation/run-two-family.sh` (new): the
  lane, environment-gated, with the pinned checkouts, the explicit peer
  selection, and the sanitized extraction.
- `scripts/check-network-participation-evidence.sh` (new): the evidence
  checker, with a `--self-test` and a negative control per rule.
- The participant roles' receipts: a stable, sanitized shape the lane can
  read (already the repository's convention).

## Ordered work packages

1. **WP1 — the lane shape.** The environment contract, the pinned
   checkouts, the peer selection, the sanitized extraction, and the
   fail-closed behavior when the environment is missing. **Amended by Plan
   495:** before any process launches, each family's **peer-eligibility
   preflight** runs — the family's RouterInfo admission gates and its own
   selection predicates, derived from the pinned source and evaluated
   offline against our lab RouterInfo. Each predicate is recorded as
   `satisfied`, `deliberately-unsatisfied` or `unknown`; an `unknown`
   predicate fails the lane closed. The lab RouterInfo profile is the one
   the gate document states, including the per-family differences: i2pd
   files us as a floodfill — the target of its exploratory requests — only
   when we advertise the floodfill capability **and** clear its floodfill
   floor (`router.version >= 0.9.62`) **and** publish a non-introducer IPv4
   address, and it learns our RouterInfo in-band; below that floor it files
   us nowhere and falls back to its generic dial path, so the dial floor and
   the peer-test floor are separate conditions and neither is implied by the
   floodfill cap. Java's profile-creation predicate
   (`ProfileManagerImpl.shouldCreate(caps)`, which `heardAbout()` calls)
   requires `R` plus either `f` or an acceptable bandwidth and congestion
   class.
2. **WP2 — the DHT leg.** Exploration and a lookup against each family.
   **Amended by Plan 495:** each leg records the gate's claim-tier ladder
   and exactly one outcome class, so a family that never selected us is
   `peer-selection-refused` and not a failed DHT leg.
3. **WP3 — the introducer leg.** A Relay Request answered as the introducer
   against each family. **Amended by Plan 495:** as WP2, and the tier that
   matters here is `T5` — the family initiating a fresh session to us — which
   is the tier that proves the family can *choose* us.
4. **WP4 — the transit leg.** Relayed transit traffic against each family.
   **Amended by Plan 495:** the reference peer for this leg must be
   **transit-capable**. The controlled transport lane runs its peer with
   `notransit = true`, which cannot satisfy this leg; changing that is a
   different peer configuration with a different profile and ban surface,
   and it gets its own attempt, its own peer-state hygiene, and its own
   evidence.
5. **WP5 — the matrix and the checker.** The evidence file's shape, the
   matrix's completeness rule, and the checker's negative controls.
   **Amended by Plan 495:** the matrix carries the tier outcomes and the
   outcome class per cell, so a cell reads `peer-selection-refused` rather
   than an unqualified "failed". The checker's negative controls include
   the deliberate-withholding case, which must classify as
   `peer-selection-refused` with the family's own reason and never as
   `i2pr-defect` and never as a pass.
6. **WP6 — the inventory and the conformance updates.** Exactly what the
   evidence earns, and nothing more.

## Failure, cancellation, restart, and contention semantics

- **A missing environment fails the lane** with a precise message naming
  what is missing; it never degrades to a skip.
- **A family that cannot be reached** is recorded as `not-reached` with the
  reason; the matrix distinguishes "failed" from "not attempted".
- **Amended by Plan 495:** every cell additionally carries one of the gate's
  outcome classes — `pass`, `i2pr-defect`, `peer-admission-refused`,
  `peer-selection-refused`, `peer-state-poisoned`, `environment-unavailable`
  or `timeout`. Only `i2pr-defect` and `pass` are statements about `i2pr`.
  A family that admits our RouterInfo but declines to select us is
  `peer-selection-refused` with that family's own exclusion reason, and is
  **not** a failed leg.
- **A partial run** leaves no partial evidence file; the evidence file is
  written atomically at the end.
- **A changed topology** (a different address, a different pin) invalidates
  the previous run and must be recorded as a new attempt with the causal
  delta.
- **Peer-state hygiene** applies per attempt and per family: a fresh peer
  data directory and a fresh peer process, no stale peer profiles, no
  mismatched static key on the live port, and the advertised endpoint equal
  to the bound socket. A peer-side ban or cooldown is `peer-state-poisoned`
  and does not consume the attempt budget.

## Compatibility and migration

- **No wire change**, **no `router.version` change**, **no migration**.
- The lane is additive tooling; it changes no production behavior.

## Required tests

- the checker's `--self-test` and one negative control per rule (an
  incomplete matrix, a non-sanitized field, a missing family row, a
  `|| true`-style early success);
- a fail-closed test for a missing environment;
- the ordinary offline suite must remain green.

## Exact verification commands

```text
bash tests/integration/network-participation/run-two-family.sh
bash scripts/check-network-participation-evidence.sh
bash scripts/check-network-participation-evidence.sh --self-test
# Plan 495 landed the gate checker and its evidence-shape validator. This
# plan's matrix cannot run before them, and they now run in the floor.
python3 scripts/plan495-peer-eligibility.py --self-test
# Per-leg artifacts are validated with:
#   python3 scripts/plan495-peer-eligibility.py --preflight --profile <file>
#   python3 scripts/plan495-peer-eligibility.py --evidence <file>
#   python3 scripts/plan495-peer-eligibility.py --hygiene <file>
cargo test --locked --workspace --all-targets -- --test-threads=1
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

## Documentation updates

- `specs/CONFORMANCE.md` — the two-family matrix and the interoperability
  status;
- `specs/support.toml` — the entries the evidence earns, and
  `m6_full_two_family_router_conformance`;
- `docs/architecture/tooling.md` — the new lane and checker;
- `plans/subsystems/network-participation-roadmap.md`,
  `plans/subsystems/mixed-router-interop-roadmap.md`, and
  `plans/registry.md` — the phase's row.

## Acceptance criteria

1. The lane runs only against the pinned checkouts and fails closed when
   the environment is missing.
2. All three participant legs are exercised against **both** families, or
   the matrix records precisely which leg failed against which family and
   why. **Amended by Plan 495:** each cell records the gate's outcome class
   and its tier ladder, so a family that declined to select us reads
   `peer-selection-refused` with that family's own reason and is never
   recorded as a failed leg or as an `i2pr` defect.
3. The evidence file contains only sanitized counts and hashes, and the
   checker enforces that.
4. The checker's negative controls all fail as expected.
5. The support inventory and the conformance record state exactly what the
   evidence proves, and no capability, version, or advertisement exceeds
   it.
6. **Amended by Plan 495:** the deliberate-withholding control classifies as
   `peer-selection-refused` with the family's own reason, and never as
   `i2pr-defect` and never as a pass; and a `peer-state-poisoned` outcome
   does not consume the attempt budget.
7. The routine floor is green on the closing tree.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the authorized topology is unavailable again — this is the expected
  outcome while Plan 431 remains stopped, and it must be recorded as
  `stopped`, not retried against an unchanged topology;
- **amended by Plan 495:** a family consistently declines to select us
  (`peer-selection-refused`) across every attempt that a peer-state reset
  could rescue. That is a **recorded outcome about the family's selection
  state**, and it is the honest end of the lane for that family — it is not
  a reason to retry unchanged, and it is not an `i2pr` defect. The closure
  records it as such;
- a family's behavior would require a production wire change to pass;
- the lane would need the reference routers patched or vendored;
- any step would require weakening a boundary script, changing a pin, or
  loosening the fail-closed semantics to go green;
- the attempt budget is spent without a material change.

## Closure evidence required

- `plans/closure/network-participation/493-status.md` with the full closure
  content the repository requires, including the matrix;
- the lane's evidence file;
- the registry and roadmap rows advanced to closed;
- the unblock audit recorded.

## Handoff notes

- **This plan is expected to stop more than once.** The topology gate has
  already stopped Plan 431; a second stop here is a recorded outcome, not a
  failure to be worked around.
- **Do not substitute a one-family lane** for the two-family requirement;
  `specs/CONFORMANCE.md` item 7 is explicit, and a one-family result is
  progression evidence, not conformance evidence.
- **Do not let a passing lane widen the claim.** The matrix says what
  worked; the support inventory must say the same thing and no more.
