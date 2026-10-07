# Plan 367 — documentation alignment after Plans 360–366

Status: **passed-eleven-stale-claims-corrected-with-closure-and-audit-snapshots-untouched**

Closure record: [`plans/closure/workspace-foundation/367-status.md`](../../closure/workspace-foundation/367-status.md)

Classification: **documentation corrective**. No production change, no new behaviour.

Subsystem: `workspace-foundation` (documentation/authority consistency).

## Origin: the mandatory unblock audit

The unblock audit required at the closure of Plans 360/361/362/364/365/366 returned a clear verdict
on unblocking — see "Audit verdict" below — and a second finding: **eleven prose claims across the
repository had become false** the moment those plans landed. Repo rules make doc-vs-source drift a
defect to correct, not to leave.

Three clusters, all the same shape: a guard or entry point was fixed, and a document kept describing
the broken state.

### A1 — `i2pr run` still described as opening no listener (4 locations)

- `plans/subsystems/router-console-roadmap.md:351-355` — "The console has **no product-reachable
  path**, because `i2pr run` does not open any listener … Closing that defect needs its own
  plan-of-record and is not part of this line." Both the premise and the forecast are now false.
- `README.md:79-80` — "`i2pr run` does **not** currently reach a serving state — see *Known
  limitation* below." **Dangling**: there is no "Known limitation" heading any more; Plan 360's
  rewrite replaced it with "Running the router".
- `README.md:134-135` — "the console is currently unreachable from the product path … (see below)",
  pointing at a section that now says the opposite **seven lines later in the same file**.
- `docs/architecture/i2pr-console.md:219-220` — "unreachable from the product path … (see
  `README.md` → 'Known limitation')". Doubly dangling.

### A2 — `check-m12-floodfill-boundaries.sh` still described as exiting 1 (5 locations)

Ground truth after Plan 364: it exits 0, has 9 traced assertions, and is in **both** the
`AGENTS.md` floor and `ci.yml`.

- `docs/architecture.md:136-142`
- `docs/architecture/i2pr-netdb.md:767-774` — including "flag it to the plan owner"
- `docs/architecture/i2pr-daemon.md:685-692` — a "Broken-script note (verified, not fixed)" that
  **survived an edit to this same file in the 360–366 batch**, sitting beside the corrected text at
  `:140-146`
- `docs/architecture/tooling.md:48` — and this one is actively harmful: it says *"**Keep it out of
  the floor until a plan corrects the rule"***, so a reader following it would **remove** a floor
  line `AGENTS.md` now requires.
- `docs/architecture/tooling.md:222-226` — plus a list header count that is now wrong.

### A3 — tooling inventory not refreshed (3 locations)

- `docs/architecture/tooling.md:209` — "25 floor checkers, 22 in `ci.yml`, 33 `check-*` files on
  disk", recomputed 2026-10-05. Now stale: **49** floor steps, and three new `check-*` files exist.
- **No inventory row at all** for `scripts/check-adr-number-uniqueness.py` (Plan 361) or
  `scripts/check-workflow-validity.py` (Plan 365), both of which are in the floor.
- `docs/architecture/tooling.md:70` says "4430 production lines"; the measured figure is **4416**
  (Plan 366's own closure record finding D5, recorded and deliberately not fixed then because the
  file was outside its scope — but it *was* edited in that batch, so the stale number survived).

## Objective

Every live document agrees with the tree. No document tells an operator or a future agent to undo a
fix.

## In scope

1. Correct A1, A2, A3 in place, at the quoted lines.
2. Add the missing inventory rows for the Plan 361 and Plan 365 guards.
3. Fix the 4430 → 4416 figure.
4. Recompute the tooling counts by a **stated method**, so the number is reproducible.

## Out of scope — and this is the important boundary

- **Plans 356/357/358 closure records are NOT rewritten.** They contain four statements that Plan
  360 made false ("no product-reachable path while `i2pr run` remains broken"). The repo's authority
  order and `registry.md`'s own "historical closure is preserved" language mean these records stay
  exactly as written. The correction is a **forward note in the console roadmap**, in the same shape
  `registry.md:139` uses for Plan 335. Rewriting them would be precisely the concealment the planning
  rules forbid.
- **Dated audit snapshots** under `docs/architecture/audit/` are point-in-time records against a
  named commit. They stay.
- **R1 (`i2pr-api` socket-keyed asymmetry) and R4 (transport grouped `std::net`)** remain **OPEN**
  and must continue to be recorded as open. R4 is already carried in `AGENTS.md`'s Known-checker-gaps
  and `tooling.md`. Neither is closed by documentation alignment.
- No production change, no guard change, no capability or advertisement claim.

## Invariants

- **No closure record is edited.** Corrections are forward notes.
- **No dated audit snapshot is edited.**
- Every corrected sentence names the plan that made it false, so a reader can check it.
- **No sentence is left telling an agent to keep a now-green guard out of the floor.**
- No new dependency, no production change.

## Required evidence

- Each of the 11 quoted locations no longer contains its stale claim, and the corrected text names
  the responsible plan.
- `grep` finds no remaining "exits 1", "no product-reachable path", "does not open any listener", or
  "Keep it out of the floor" phrasing across `README.md`, `AGENTS.md`, `docs/`, and `plans/`
  (excluding closure records and dated audit snapshots, which are exempt by rule).
- `docs/architecture/audit/` and `plans/closure/` are byte-identical to their pre-plan state.
- The tooling counts are recomputed with a stated method that a reader can repeat.
- The planning checkers and the new guards still pass.

## Production changes

None. Documentation and authority files only.

## Acceptance criteria

Plan 367 passes only when:

1. all 11 stale claims are corrected at the quoted locations;
2. no closure record and no dated audit snapshot was modified — asserted by `git diff` over
   `plans/closure/` and `docs/architecture/audit/`, both of which must be empty;
3. the two new guards have inventory rows;
4. R1 and R4 remain recorded as **open**;
5. exact-head routine CI is green.

## Stop conditions

Stop and record a classified boundary if correcting any claim would require editing a closure record
or an audit snapshot.

## Closure evidence required

Commits; the 11-location before/after table; commands with local/CI outcomes labelled truthfully;
the `git diff` emptiness proof for `plans/closure/` and `docs/architecture/audit/`; the recount with
its stated method; known limitations; findings by severity; roadmap disposition.

## Audit verdict — recorded here so it is not lost

**Closing Plans 360–366 unblocks nothing.** No implementation plan and no other closure record names
any of the six as a hard or interface dependency; the only references are the six closure records,
the six registry rows, and two roadmap rows. Every currently-blocked plan keeps an unrelated blocker:

| Plan | Remaining blocker | Touched by 360–366? |
|---|---|---|
| 348 | 347 alone | no |
| 347 | Java bandwidth tier (ADR 0030) — `stopped` at a classified boundary | no |
| 326 | 347 | no |
| 327 | an outproxy that actually answers; no Java/i2pd outproxy exercised | no |
| 325 | no qualified maintained Red25519 provider | no |
| 328 | superseded forward by 348 | no |
| 278 | reference client rejects the controlled RouterInfo — budget spent | no |
| 279 | hard dep Plan 278, still unmet | no |
| 306 | truthful bandwidth-class design | no |
| 308 | controlled HTTP topology + three family captures | no |
| 313 | already `ready`; external i2pd lane | no |
| 201/247 | live-lane integration gap | no |

No state transition to `ready` is warranted, and no corrective needs registering for an unblock. The
"never silently unblock" rule is satisfied by recording that nothing cleared.