# Plan 303 — M12 matrix execution with an unseeded publisher trigger

Status at registration: **registered-m12-matrix-execution-with-unseeded-publisher-trigger**

Classification: narrow lane-semantics corrective + matrix execution. Follows the
Plan 302 pass (`plans/closure/floodfill/302-status.md` §§2–3).

Hard dependencies: Plan 302 passed (wire form proven live, idempotency mechanism
proven). Plans 278/284/285 unchanged (stopped/retained/retained); Plan 279 stays
blocked until this plan executes the matrix.

## 1. Objective

Execute the Plan 278 matrix A–I against exact-pinned i2pd under a fresh frozen
budget, with the single lane change Plan 302 §3 requires: the publisher key must
NOT be pre-seeded, so the reference client's first publication inserts (rather
than re-states) and offers a replication candidate for matrix F.

## 2. Why-ready (what Plan 302 proved)

- Floodfill replies in short-transport form are consumed live by the reference
  (`Publishing confirmed`, zero expired drops, lookup record answers consumed).
- An idempotent publisher re-store is acked without replication — correct
  behavior, proven locally and observed live. No planning defect exists.
- The only blocker is structural: the lane seeds all three reference keys
  including the publisher, making matrix F's trigger impossible.

## 3. Lane change (explicit, minimal, reviewed here — not smuggled in)

- The `#[ignore]`-gated driver seeds exactly the two floodfill peers (A, B) as
  replication targets and lookup fixtures. It does NOT seed the reference
  client (C) key.
- Matrix C/E lookup fixtures must be re-examined: if any row depends on C's
  record being answerable before C publishes, the plan must say so before the
  attempt (C publishes at startup, so its record exists from matrix A onward;
  rows that need it earlier must be ordered after A or documented blocked).
- Nothing else changes: same topology, same pins, same budgets shape, same
  sanitized evidence convention. No production code change is expected; if the
  attempt exposes one, stop and register the next narrow corrective.

## 4. Evidence principles (same as Plans 278/302)

- Exact pin i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`, verified
  before execution; loopback-only; reseed disabled; no public network.
- Fresh frozen budget of 1, recorded before execution. Spent budgets (278, 302)
  are not widened.
- External rows `#[ignore]`-gated, fail-closed without env. Raw reference logs
  diagnostic only; evidence is sanitized counts/hashes/categories.
- No retry-until-green. First publisher store must show
  `outcome=[inserted|replaced]` with `replication_offered=1`, or the attempt
  stops with the exact observation (do not re-seed mid-lane to force it).

## 5. Acceptance criteria

- Matrix A–I rows recorded exactly as observed; zero rows claimed beyond what
  the attempt shows.
- `publisher-store-insert-outcome` shows a non-idempotent first insert with a
  planned DirectFlood, or the exact contrary observation is recorded.
- Exact-head ordinary CI green; boundary scripts green.
- No capability/version/RouterInfo advertisement change.

## 6. Stop conditions

Stop on any reproducible i2pr protocol defect (register the next narrow
corrective), on reference-lane unavailability (record the exact boundary), or
at budget exhaustion. Never increase the budget, patch i2pd, broaden network
access, or re-seed mid-lane.

## 7. Closure evidence required

Driver diff (seed change only), one bounded attempt (pin metadata, frozen
budget record, sanitized evidence incl. insert-outcome row, expired-drop
count, `Publishing confirmed` presence, matrix rows), CI run,
security/resource review, unblock audit.

## 8. Handoff

If the matrix passes, Plan 279 (second-family qualification) becomes
unblocked to ready — it does not auto-pass. If it stops, the next corrective
owns the finding. Plan 278 remains stopped as a historical record regardless.
