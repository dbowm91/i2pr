# Plan 367 — closure

Status: **passed-eleven-stale-claims-corrected-with-closure-and-audit-snapshots-untouched**

Implementation plan:
[`plans/implementation/workspace-foundation/367-doc-alignment-after-360-366.md`](../implementation/workspace-foundation/367-doc-alignment-after-360-366.md)

Classification: **documentation corrective**. No production change, no guard change, no
new behaviour.

Subsystem: `workspace-foundation`.

## Commits

| Commit | Content |
|---|---|
| `2f82c799` | The eleven corrections, across 9 files: `README.md`, `AGENTS.md`-adjacent skill text, `docs/architecture.md`, `i2pr-console.md`, `i2pr-daemon.md`, `i2pr-netdb.md`, `tooling.md`, `plans/registry.md`, `plans/subsystems/router-console-roadmap.md` |
| Plan 372 `a6d82299`…`df2e7d5c` | Supplied acceptance criterion 5, plus corrected four claims Plan 367 had left in place |

## Origin

The mandatory unblock audit required at the closure of Plans 360/361/362/364/365/366
returned a clear verdict on unblocking, and a second finding: **eleven prose claims across
the repository had become false** the moment those plans landed. Repo rules make
doc-vs-source drift a defect to correct, not to leave.

## Objective met

Every live document agrees with the tree, and no document tells an operator or a future
agent to undo a fix.

### The eleven locations

**A1 — `i2pr run` still described as opening no listener (4 locations).** Ground truth after
Plan 360: `i2pr run` starts, binds its configured loopback listeners, and shuts down
cleanly.

| Location | Correction |
|---|---|
| `plans/subsystems/router-console-roadmap.md:351-355` | premise and forecast both false; corrected forward |
| `README.md:79-80` | **dangling** — pointed at a *Known limitation* heading Plan 360 had already removed |
| `README.md:134-135` | **self-contradicting** — pointed at a section seven lines later saying the opposite |
| `docs/architecture/i2pr-console.md:219-220` | **doubly dangling** — same removed heading |

**A2 — `check-m12-floodfill-boundaries.sh` still described as exiting 1 (5 locations).**
Ground truth after Plan 364: exit 0, nine traced assertions, in both the `AGENTS.md` floor
and `ci.yml`.

| Location | Correction |
|---|---|
| `docs/architecture.md:136-142` | corrected |
| `docs/architecture/i2pr-netdb.md:767-774` | corrected, including the "flag it to the plan owner" note |
| `docs/architecture/i2pr-daemon.md:685-692` | corrected — a "Broken-script note (verified, not fixed)" that **survived an edit to this same file** in the 360–366 batch, sitting beside the corrected text at `:140-146` |
| `docs/architecture/tooling.md:48` | **actively harmful** — instructed the reader to *"Keep it out of the floor until a plan corrects the rule"*, so a reader following it would **delete a floor line `AGENTS.md` requires** |
| `docs/architecture/tooling.md:222-226` | corrected, with a stale list-header count |

**A3 — tooling inventory not refreshed (3 locations).** Corrected, with counts recomputed
under two stated methods.

## Invariants held

- **No closure record was edited.** The **Plan 356/357/358 closure records are not
  rewritten**: they assert the pre-360 posture, and the correction is a forward note in
  the console roadmap, in the same shape `registry.md` uses for Plan 335. Rewriting them
  would be exactly the concealment the planning rules forbid.
- **No dated audit snapshot was edited.** `docs/architecture/audit/` is point-in-time
  against a named commit and stays byte-identical.
- Every corrected sentence names the plan that made it false, so a reader can check it.
- No sentence was left telling an agent to keep a now-green guard out of the floor.
- No new dependency, no production change, no capability or advertisement claim.

## Acceptance criteria

| # | Criterion | Result |
|---|---|---|
| 1 | All 11 stale claims corrected at the quoted locations | **pass** — `2f82c799` |
| 2 | No closure record and no audit snapshot modified | **pass** — `git show --name-only --format='' 2f82c799 \| grep -E '^\^(plans/closure/\|docs/architecture/audit/)' \| wc -l` → **0** |
| 3 | The two new guards have inventory rows | **pass** — `tooling.md` rows for `check-adr-number-uniqueness.py` and `check-workflow-validity.py`, both `Floor: yes` / `CI: yes` |
| 4 | R1 and R4 remain recorded **open** | **pass** — `362-status.md:324,327`; R4 also in `AGENTS.md` Known-checker-gaps |
| 5 | Exact-head routine CI is green | **pass, but only after Plan 372** — see below |

### Criterion 5 was not reachable without another plan

Plan 367's criterion 5 requires exact-head routine CI to be green. At registration it was
not, and no local action could change that: `Quality (macos-latest)` was red because
`check-workflow-validity.py` — the guard Plan 365 had added to the floor — needed PyYAML
and nothing installed it, so it exited 2 by design on every macOS run. That defect, and
the fact that `main` was red for the same reason, were outside Plan 367's scope.

Plan 372 provisioned the dependency and **unblocked this criterion**: CI run **`37585830152`
on `df2e7d5c` is green on all four jobs**, including the macOS leg that had been red since
Plan 365 landed.

Recording this rather than claiming criterion 5 was met by Plan 367 alone is the point:
a plan that blocks on an environmental defect should say so, and the corrective should be
its own plan with its own record.

## Re-verification on the closing tree

All five criteria were re-checked against `df2e7d5c` rather than assumed from `2f82c799`:

| Criterion | Re-check |
|---|---|
| 1 | `grep` for `does not open any listener`, `no product-reachable path`, `Keep it out of the floor` finds hits only inside Plan 367's own text (which quotes its origin) and Plan 364's plan doc (which quotes its own premise) — both legitimate |
| 2 | `git diff --name-only a6d82299~1..HEAD -- plans/closure/ docs/architecture/audit/` → **0 files** |
| 3 | present |
| 4 | present |
| 5 | run `37585830152`, success |

## Verification commands

| Command | Result |
|---|---|
| `git show --name-only --format='' 2f82c799 \| grep -cE '^\^(plans/closure/\|docs/architecture/audit/)'` | **0** |
| stale-phrase sweep across `README.md`, `AGENTS.md`, `docs/`, `plans/` excluding `plans/closure/` and `docs/architecture/audit/` | only self-quoting plan text |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 47 tests |
| `python3 scripts/check-workflow-validity.py` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| Full `AGENTS.md` routine floor | **54/54 PASS** locally |
| CI run `37585830152` on `df2e7d5c` | **success** (CI) |

## Findings recorded rather than fixed

- **R1** — the `i2pr-api` rule set is socket-keyed with `std::net` *values* permitted, so
  a bare `use std::{net};` with no socket use is not flagged. Verified by probe, exit 0.
  **OPEN.** Tightening it would fire on legitimate address-value imports elsewhere.
- **R4** — the transport crates already import `std::net` address values via grouped `use`
  (`i2pr-transport-ntcp2/src/address.rs:9` and four ssu2 files), and the transport scan was
  deliberately excluded from the normalised pass for that reason. **OPEN**, and
  `AGENTS.md` records that a transport-scoped plan should settle the socket-keyed rule
  there the way Plan 366 did for the console.

Neither is a documentation defect; neither is closed by this plan.

## Plan 372's corrections to claims Plan 367 left

Plan 367 fixed the eleven locations it named and missed three more instances of the same
A1 claim plus a fourth surface of the A3 count drift. Plan 372 corrected:

- `plans/registry.md` *Blocked work* — Plan 369's row still read `active … WP5–WP6 open`.
- `plans/registry.md:40` and `:315`, and `plans/subsystems/router-console-roadmap.md:3` —
  three surviving copies of "the console is not reachable through `i2pr run`".
- the `tooling.md` *Inventory at a glance* table, the workspace roster, the MSRV job
  version, the fuzz-target count, and the false "zero Rust integration files" claim.

The lesson is the same one this plan was created to record, and it is now enforced rather
than remembered: Plan 372 added `scripts/check-tooling-inventory.py`, which fails closed
when a published count stops matching the tree.

## Unblock audit

Performed at registration and re-performed here. **Closing Plans 360–366 unblocks nothing
else.** No implementation plan and no other closure record names any of the six as a hard
or interface dependency; the only references were the six closure records, the six registry
rows, and two roadmap rows. Every currently-blocked plan keeps an unrelated blocker — the
full table is in `372-status.md`. No state transition to `ready` is warranted.

Plan 367's own blocking relationship runs the other way: Plan 372 was registered to supply
this plan's criterion 5.

## Roadmap disposition

`plans/subsystems/workspace-foundation-roadmap.md` gains a Plan 367 row. Its milestones stay
closed; the lane fixes tooling and one product-path defect without reopening Milestones 1
or 2.

## Known limitations

- Plan 367 corrected the eleven locations it enumerated. It did not sweep the whole drift
  class, and three further instances of its own A1 claim survived it until Plan 372 found
  them. This is now caught mechanically rather than by enumeration.
- The R1/R4 residuals remain open by design; see above.
- `docs/architecture/audit/` still contains point-in-time snapshots that no longer match
  the tree. That is their purpose, and they are excluded from the drift guard by rule.

## Handoff

Nothing further is required for Plan 367. The next plan that changes the workspace shape
must update `tooling.md`, because `check-tooling-inventory.py` will fail until it does.