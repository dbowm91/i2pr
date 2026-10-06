# Plan 365 — CI workflow validity guard: status

Status: **passed-workflow-validity-now-checked-in-the-routine-floor**

Plan of record:
[`365-workflow-validity-guard.md`](../../implementation/workspace-foundation/365-workflow-validity-guard.md).
Origin: found while executing Plan 364, whose author needed to add a `ci.yml` step and could not,
because the file did not parse.

## The defect, and who caused it

`.github/workflows/ci.yml` **did not parse as YAML at `2416c30`**. One step line sat at column 0
among six-space-indented siblings:

```yaml
      - name: Check runtime boundaries
        if: runner.os == 'Linux'
        run: bash scripts/check-runtime-boundaries.sh
- name: Check managed-app private client seams      # <-- column 0
        run: python3 scripts/check-managed-app-private-client-seams.py
```

Bisected by parsing each commit's blob:

| Commit | Parses? |
|---|---|
| `2716aa1`, `73506b3`, `f49605b`, `26155be` | yes |
| `0d50319` — merge of `origin/work/plans-352-353` | **no** |
| `2416c30` — then `origin/main` | **no** |

The source branch `origin/work/plans-352-353` **parses correctly**. The break was introduced by the
**merge resolution**, which dropped six spaces. **This was my own regression from the preceding
session's six-branch merge, not an inherited defect.** It is recorded that way rather than
attributed to a predecessor.

An unparseable workflow means GitHub rejects the whole file, so the `quality`, `msrv`, and
`dependency-policy` jobs **never ran**, and every `check-*.sh` / `check-*.py` step inside `ci.yml`
was inert. Any closure record citing "exact-head routine CI green" for that window is weaker than it
reads. That statement is about historical records and is left as an observation, not a rewrite.

## Why the floor could not see it

The routine floor runs `cargo`, shell guards, and Python guards. **Not one of the 46 original steps
parses YAML.** Every one of them passed on a workflow that could not run. The gap was structural,
not accidental, so this plan adds the missing parse step rather than just repairing one line.

## What was delivered

- **New `scripts/check-workflow-validity.py`.** Parses every `.github/workflows/*.yml`, fails closed
  on a parse error reporting **file, line, column and the parser message**, and adds structural
  assertions that catch merge damage in terms a reviewer recognises:
  - every workflow declares an `on:` trigger and at least one `job:`;
  - every job has `steps:` or `uses:`; every step has a `name:` and a `run:` or `uses:`;
  - no duplicate step `name:` within a job (which makes CI logs ambiguous about what failed).
- **New `tests/planning/test_workflow_validity.py`** — 15 rows, temp-dir fixtures throughout; the
  real `.github/workflows/` is only ever read.
- **One new floor step** in `AGENTS.md`.

PyYAML 6.0.1 is already present and is used with `safe_load`. If the import ever fails, the guard
**exits 2 with an explanatory message** rather than reporting success — a guard that silently
passes on an unchecked tree is precisely the defect class this plan exists to close.

## Requirement → evidence

| Requirement | Evidence | Result |
|---|---|---|
| In-scope 1: parse every workflow, fail closed with file/line | `scripts/check-workflow-validity.py`; `test_deindented_step_fails` | met |
| In-scope 2: structural assertions beyond parsing | `test_missing_on_trigger_fails`, `test_no_jobs_fails`, `test_step_with_neither_run_nor_uses_fails`, `test_unnamed_step_fails`, `test_duplicate_step_name_fails`, `test_non_mapping_root_fails` | met |
| In-scope 3: added to the routine floor | `AGENTS.md` step 38 | met |
| In-scope 4: negative tests, temp fixtures only | 9 negative rows, none mutating the repo | met |
| Invariant: no workflow file modified by this plan | `git diff --stat .github/workflows/` — unchanged by this plan | met |
| Invariant: PyYAML-unavailable path fails loudly | `test_main_returns_two_when_guard_cannot_run` | met |
| Re-introducing the exact defect fails, naming file and line | `test_deindented_step_fails` asserts both | met |
| AC3 (mutation-tested, recorded) | 7/7 below | met |
| **AC6 (exact-head routine CI green)** | **no CI reachable; and CI could not have been green while this file was unparseable** | **UNPROVEN** |

## Commands (all local; no CI available)

```text
python3 scripts/check-workflow-validity.py
  -> CI workflow validity: 10 workflow files parse and are structurally valid   exit 0

python3 -m unittest discover -s tests/planning -p 'test_workflow_validity.py'
  -> Ran 15 tests ... OK                                                       exit 0

python3 -m unittest discover -s tests/planning -p 'test_*.py'
  -> Ran 30 tests ... OK                                                       exit 0

bash scripts/check-workflow-validity.py
  -> exit 2   (the documented trap: these scripts must be run with python3)
```

The `bash` trap is confirmed rather than assumed: `bash scripts/check-workflow-validity.py` exits 2.

## Negative tests

| Row | Fixture | Result |
|---|---|---|
| control | the repository's 10 real workflows | pass, zero violations |
| control | minimal valid workflow | pass |
| control | reusable-workflow job with `uses:` and no steps | pass |
| **the regression** | `theme.rs`-style step de-indented to column 0 | fails, names `ci.yml:<line>:<col>` and the YAML error |
| malformed | unbalanced flow sequence | fails |
| missing trigger | no `on:` | fails |
| no jobs | empty `jobs:` | fails |
| step with no action | `name` but neither `run` nor `uses` | fails |
| unnamed step | `run:` with no `name:` | fails |
| duplicate step name | two steps named `Same` | fails |
| non-mapping root | a YAML list | fails |
| fail-closed | missing `.github/workflows` dir | raises, exit 2 |
| fail-closed | empty workflow dir | raises, exit 2 |
| exit-code mapping | violation present | `main()` returns 1 |
| exit-code mapping | guard cannot run | `main()` returns 2 |

## Mutation testing (7/7 caught)

| # | Mutation | Caught |
|---|---|---|
| M1 | swallow parse errors (`continue`) | yes |
| M2 | drop the duplicate-step-name check | yes |
| M3 | drop the `run`/`uses` check | yes |
| M4 | drop the `on:` trigger check | yes |
| M5 | `main()` returns 0 on violations | **yes — after the test was added** |
| M6 | drop the no-jobs check | yes |
| M7 | drop the unnamed-step check | yes |
| C | control restored | pass |

**Three of my own errors were found and fixed during this, and all three are worth recording:**

1. **The first mutation run reported 5/5 ESCAPED — a false result.** The harness put mutants in
   `/tmp` and passed them via `PYTHONPATH`, but the test module loads the guard by absolute path
   from the repo, so no mutant was ever executed. The rerun mutates in place with a guaranteed
   restore, and the guard file was verified byte-identical to its backup afterwards.
2. **M5 genuinely escaped** because every other row exercises the pure `check_workflows()` function
   rather than the process exit code. A row for `main()`'s violation→exit-1 mapping was added.
3. **That new row did not run.** It was written at module scope instead of inside the test class, so
   discovery silently skipped it — the count stayed at 14. Caught because the test count did not
   move, and fixed by indenting the method into the class (now 15).

An escaped mutation and a test that never ran are the same failure wearing different clothes. The
count check is what caught the second.

## Findings by severity

| ID | Sev | Finding | Disposition |
|---|---|---|---|
| W1 | **critical** | `ci.yml` unparseable → `quality`/`msrv`/`dependency-policy` never ran | line repaired under Plan 364; this plan prevents recurrence |
| W2 | **high** | no floor step parses YAML, so CI-claiming defects are invisible | guard added to the floor |
| W3 | medium | duplicate step names silently make CI logs ambiguous | new assertion |
| W4 | low | step with neither `run:` nor `uses:` was accepted | new assertion |
| W5 | info | PyYAML is an environment dependency, not a declared one | guard fails loudly (exit 2) rather than passing; noted below |

## Known limitations

1. **AC6 is unproven** — no CI is reachable. Compounding this, CI *could not* have been green while
   `ci.yml` was unparseable, so the criterion has a prerequisite that only a real run can satisfy.
2. **PyYAML is not a declared dependency.** It is present in this environment (6.0.1) and used with
   `safe_load`. If a future floor environment lacks it, the guard exits 2 with an explanation rather
   than passing — deliberately, but it is a floor step that can fail for an environmental reason.
   Adding it as a declared dev-dependency needs a dependency-review decision and is **not** done here.
3. **This validates syntax and shape, not semantics.** It does not audit whether the existing steps
   are the *right* steps, nor whether any historical CI run was green. That needs a real CI run.
4. **It does not lint action pins, permissions, or concurrency** — out of scope by design.
5. **No `ci.yml` repair is claimed by this plan.** The one-line repair was Plan 364's, because its
   own step would have been inert without it. Both facts are recorded rather than conflated.

## Verification batching — stated plainly

Plans 360, 361, 362, 364, 365, and 366 executed together. Targeted verification ran per plan; the
**full `AGENTS.md` routine floor ran once on the combined tree** (49 steps). No per-plan floor run
is claimed.

## Roadmap disposition

`plans/subsystems/workspace-foundation-roadmap.md` gains Plan 365 as passed. `AGENTS.md`'s Known
checker gaps now records that CI-workflow validity was previously unchecked and names the concrete
break. No capability, advertisement, or support-surface claim changes.