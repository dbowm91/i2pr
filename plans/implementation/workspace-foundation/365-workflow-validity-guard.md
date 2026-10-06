# Plan 365 — CI workflow validity guard

Status: **registered-merge-regression-that-no-floor-step-could-detect**

Classification: **invariant corrective**. Found while executing Plan 364.

Subsystem: `workspace-foundation` (CI tooling).

## The defect, and how it was found

While adding Plan 364's step to `.github/workflows/ci.yml`, that file was found to **not parse as
YAML at all**. Line 84 had zero indentation where every sibling step had six:

```yaml
      - name: Check runtime boundaries
        if: runner.os == 'Linux'
        run: bash scripts/check-runtime-boundaries.sh
- name: Check managed-app private client seams      # <-- column 0
        run: python3 scripts/check-managed-app-private-client-seams.py
```

Verified by bisection:

| Commit | Parses? |
|---|---|
| `2716aa1`, `73506b3`, `f49605b`, `26155be` | yes |
| `0d50319` (merge of `origin/work/plans-352-353`) | **no** |
| `2416c30` (= `origin/main` at the time) | **no** |

The source branch `origin/work/plans-352-353` **parses fine**. The break was introduced by the
**merge resolution**, which dropped six spaces of indentation on that line. This was the author's
own regression from the preceding session, found incidentally, not by a guard.

## Why it matters more than a formatting nit

An unparseable workflow means the whole `ci.yml` is rejected — **the `quality`, `msrv`, and
`dependency-policy` jobs never ran.** Every `check-*.sh` / `check-*.py` step in it was inert, and
every closure record citing "exact-head routine CI green" for that window is weaker than it reads.

The deeper problem is structural: **nothing in the 46-step routine floor parses YAML.** The floor
runs `cargo`, shell guards, and Python guards, and every one of them passed while the workflow was
unrunnable. A whole class of CI-claiming defects is therefore invisible to the floor.

## Why ready

- No hard dependency, no interface dependency, no new dependency beyond what is already available.
- Reads only `.github/workflows/*.yml`; changes no workflow semantics.

## Objective

An unparseable or structurally invalid workflow file fails the routine floor, naming the file, the
line, and the parser error.

## In scope

1. **`scripts/check-workflow-validity.py`** — parse every `.github/workflows/*.yml`, fail closed on
   a parse error with file, line and message.
2. **Structural assertions beyond "it parses"**, because those are what catch merge damage:
   - every workflow declares `on:` and at least one `job:`;
   - every step within a job has a `name` and a `run:` or `uses:`;
   - no step sits at the wrong indentation (the actual defect — caught structurally, not only by the
     parser);
   - no duplicate `name:` within a single job's steps, which silently makes CI logs ambiguous.
3. **Add it to the `AGENTS.md` routine floor**, so the gap cannot reopen.
4. **Negative tests** using temp fixtures: malformed YAML, a workflow with no jobs, a step missing
   `run`/`uses`, a mis-indented step, a duplicate step name.

## Out of scope

- **Changing any workflow's steps, triggers, or permissions.** This plan reads workflows; it does
  not edit them. The one-line `ci.yml` repair belongs to Plan 364, which needed it for its own
  step to be meaningful, and is recorded there.
- **Auditing whether the existing steps are the *right* steps**, or whether CI is actually green on
  any historical run. Those need a real CI run, which this environment does not have.
- **`ci.yml` semantic policy** — pinning actions, permissions, or concurrency. Separate.

## Invariants

- **Fails closed.** A workflow it cannot parse is an error, never a skip.
- **No workflow file is modified by this plan.**
- **No new dependency.** Python 3 standard library only. If PyYAML is unavailable, the guard must
  fail loudly rather than silently pass — that is the exact class of defect being fixed.
- The guard must not require network access or GitHub credentials.

## Required evidence

- The guard passes on all ten workflow files in `.github/workflows/`.
- Re-introducing the exact Plan-364-era defect (de-indenting a step) **fails**, naming file and line.
- Malformed YAML fails with the parser's file/line/message.
- A workflow with no jobs, a step missing `run`/`uses`, and a duplicate step name each fail.
- **Mutation-tested**: break the guard at least 4 ways and confirm the tests catch each.

## Production changes

New `scripts/check-workflow-validity.py`; new `tests/planning/test_workflow_validity.py`; one new
line in the `AGENTS.md` floor.

## Documentation updates

- `AGENTS.md` — floor step; and the Known-checker-gaps section gains a note that CI-workflow validity
  was previously unchecked.
- `docs/architecture/tooling.md` — the checker inventory.

## Acceptance criteria

Plan 365 passes only when:

1. the guard exists, is in the routine floor, and passes on the real tree;
2. it is negative-tested with recorded transcripts, including the de-indented-step case;
3. it is mutation-tested with a recorded transcript;
4. **no workflow file is modified by this plan**;
5. the PyYAML-unavailable path fails loudly rather than passing;
6. exact-head routine CI is green — **and this criterion now has a prerequisite**: CI must parse
   before it can be green, which is why this guard is in the floor at all.

## Stop conditions

Stop and record a classified boundary if PyYAML is not available in the floor environment and cannot
be added without a new dependency decision, or if any existing workflow file is found to be
structurally invalid in a way this guard cannot distinguish from intentional syntax.

## Closure evidence required

Commits; requirement-to-evidence matrix; commands with local/CI outcomes labelled truthfully; the
negative-test and mutation-test transcripts; known limitations; findings by severity; roadmap
disposition.

**Note on verification batching.** Executed alongside Plans 360, 361, 362, and 364. Targeted
verification runs per plan; the full routine floor runs once on the combined tree, and the closure
record must say so rather than implying a per-plan floor run.