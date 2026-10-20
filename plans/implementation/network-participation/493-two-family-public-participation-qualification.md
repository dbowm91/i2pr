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
   fail-closed behavior when the environment is missing.
2. **WP2 — the DHT leg.** Exploration and a lookup against each family.
3. **WP3 — the introducer leg.** A Relay Request answered as the introducer
   against each family.
4. **WP4 — the transit leg.** Relayed transit traffic against each family.
5. **WP5 — the matrix and the checker.** The evidence file's shape, the
   matrix's completeness rule, and the checker's negative controls.
6. **WP6 — the inventory and the conformance updates.** Exactly what the
   evidence earns, and nothing more.

## Failure, cancellation, restart, and contention semantics

- **A missing environment fails the lane** with a precise message naming
  what is missing; it never degrades to a skip.
- **A family that cannot be reached** is recorded as `not-reached` with the
  reason; the matrix distinguishes "failed" from "not attempted".
- **A partial run** leaves no partial evidence file; the evidence file is
  written atomically at the end.
- **A changed topology** (a different address, a different pin) invalidates
  the previous run and must be recorded as a new attempt with the causal
  delta.

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
   why.
3. The evidence file contains only sanitized counts and hashes, and the
   checker enforces that.
4. The checker's negative controls all fail as expected.
5. The support inventory and the conformance record state exactly what the
   evidence proves, and no capability, version, or advertisement exceeds
   it.
6. The routine floor is green on the closing tree.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the authorized topology is unavailable again — this is the expected
  outcome while Plan 431 remains stopped, and it must be recorded as
  `stopped`, not retried against an unchanged topology;
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
