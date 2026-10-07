# Plan 372 — provision the CI guard dependencies, and make inventory drift fail closed

Status: **registered-ci-guard-dependency-unprovisioned-and-inventory-counts-rotted-again**

Classification: **invariant** (CI provisioning + a guard that makes doc-vs-source drift
detectable) plus **documentation corrective**. No product behaviour change.

Subsystem: `workspace-foundation` (CI/tooling authority consistency).

Hard dependencies: none. Plan 367's implementation landed at `2f82c799`; this plan
corrects what that landing and Plan 369 left behind. Plan 367's own closure is blocked on
acceptance criterion 5 (exact-head routine CI green), which this plan unblocks.

## Origin

Two findings, both reached by execution rather than by reading.

### A — the CI guard's own dependency is unprovisioned, so CI has been red on `main`

Plan 365 added `scripts/check-workflow-validity.py`, which needs PyYAML and **deliberately
exits 2** rather than reporting success when the import fails. That is the correct posture
and it is unchanged here. The defect is that **nothing ever installs PyYAML**.

Measured on the exact-head runs for this branch and for `main`:

| Run | Commit conclusion | Failing job | Cause |
|---|---|---|---|
| `37574479845` (this branch) | failure | `Quality (macos-latest)` | `ModuleNotFoundError: No module named 'yaml'` |
| `37519809981` (`main`) | failure | `Quality (macos-latest)` | same |

`ubuntu-latest` passes because its image ships PyYAML; `macos-latest` does not. The
`Quality (macos-latest)` job runs `python3 -m unittest discover -s tests/planning`, and
`tests/planning/test_workflow_validity.py` reports **11 errors + 1 failure** there — the
guard's own behaviour under a missing dependency, correctly failing closed, and the suite
correctly refusing to call that a pass.

**Consequence: the Plan 365 guard has never passed in CI on macOS.** It is in the routine
floor, so it has been run locally and reported green there; nothing in any floor step
asserts that the *environment* can execute it. That is exactly the defect class Plans
360–366 exist to close, one level up: not a guard that enforces nothing, but a guard whose
own precondition was never provisioned.

`ci.yml` contains no `pip install`, no `setup-python`, and no dependency declaration of any
kind. There is no `requirements.txt` in the repository, and no other script imports `yaml`.

### B — Plan 367's inventory counts rotted within one commit

Plan 367's objective was "every live document agrees with the tree". It recomputed
`docs/architecture/tooling.md`'s inventory to fix a triple that was "not reproducible under
either method". Plan 369 then landed and the counts were stale again — **and the same
recurrence had already happened for a third count surface Plan 367 never recomputed.**

Every figure below is measured on this tree. Method A figures are reproduced with the
exact commands Plan 367 published, so a reader can re-run them.

| Location | Claim | Actual | Method |
|---|---|---:|---|
| `tooling.md:16` | Top-level `scripts/` files: 35 | 53 | `ls -1 scripts/*.* \| wc -l` |
| `tooling.md:16` | …of which `check-*`: 33 | 51 | `ls scripts/check-* \| wc -l` |
| `tooling.md:18` | `check-*` on disk, all classes: 36 | 54 | `git ls-files 'scripts/check-*'` |
| `tooling.md:18` | …33 top level + 3 under `scripts/interop/` | 51 + 3 | same |
| `tooling.md:19` | Checker invocations in `ci.yml`: 24 | **34** | count `- name:` + `run:` steps naming a checker |
| `tooling.md:21` | `check-*` on disk: 49 (56 with interop) | 54 (57 with interop) | `git ls-files` |
| `tooling.md:22` | Fuzz targets: 24 | **25** | `grep -c '^\[\[bin\]\]' fuzz/Cargo.toml` |
| `tooling.md:249-252` | floor steps invoking a checker: 40 | 41 | Plan 367's own `sed \| grep -c` |
| `tooling.md:250` | Total routine-floor steps: 49 | **51** | same block, `grep -cE '^(cargo\|python3\|bash\|RUSTDOCFLAGS)'` |
| `tooling.md:251` | Checkers executed by `ci.yml`: 30 | 32 | Plan 367's own `grep -oE … sort -u` |
| `tooling.md:252` | `check-*` on disk: 49 | 51 | `ls scripts/check-*` |
| `tooling.md:857` | Workspace members: 19 crates + 1 tool = 20 | **25 + 1 = 26** | `cargo metadata --no-deps` |
| `tooling.md:870` | `rust-version = "1.88"` | `1.89` | root `Cargo.toml` |
| `tooling.md:703,971` | MSRV job runs 1.88.0 / "verification runs 1.88.0" | **1.89.0** | `.github/workflows/ci.yml:181` |
| `tooling.md:~973` | "**Zero** Rust integration test files under `tests/`" | **false** — `tests/portable-service-tunnel-consumer/tests/conformance.rs` is tracked | `git ls-files tests \| grep '\.rs$'` |
| `overview.md:38` | "20 workspace crates plus one non-production launcher tool" | 25 + 1 | `cargo metadata --no-deps` |

Two of these are **more than a stale digit**, and both are the class that actually misleads:

- **`tooling.md:703` names the wrong MSRV job version.** Plan 357 raised the floor
  `1.88 → 1.89` because every published `eggserve-server` declares `rust-version = "1.89"`.
  `AGENTS.md`, the root `Cargo.toml`, `ci.yml`, and `i2pr-daemon.md` all say 1.89;
  `tooling.md` still documents the CI job as running 1.88.0. A reader would conclude the
  MSRV job verifies a toolchain the project does not support.
- **"Zero Rust integration test files under `tests/`" is false**, and false in the direction
  that hides a real dependency edge: the portable service-tunnel consumer crate is
  exercised through a tracked `tests/` integration file. The claim is load-bearing for
  "all verification lives inside the crates".

`tooling.md:16-24` also mixes counted and uncounted conventions in one table — `ls`-based
figures include gitignored `__pycache__/*.pyc`, while the `git ls-files` figures do not. The
69-file `scripts/interop/` figure is **correct** (Plan 367 did not get that one wrong), and
`scripts/interop/__pycache__/` is gitignored — so an unfiltered `find` reports 86 where the
real answer is 69.

### Why counting by hand cannot hold

Three separate surfaces in one document published absolute counts of a tree that changes
every time a guard or a lane is added. Each was correct when written. Plan 367 fixed one,
two more rotted, and Plan 367 never recomputed the `Inventory at a glance` table at all.
**A human recount is the mechanism that produced the defect.**

The corrective is therefore not "recount more carefully". It is: publish the counts with a
**stated, mechanically re-runnable method** (Plan 367's contribution, kept), and add a guard
that **fails closed when a published count stops matching the tree** (this plan's
contribution).

## Objective

1. `ci.yml` provisions PyYAML on every runner that executes a guard needing it, so the
   Plan 365 guard actually runs in CI and `Quality (macos-latest)` can go green.
2. Every published inventory figure in `docs/architecture/tooling.md` and
   `docs/architecture/overview.md` matches the tree, recomputed by a stated method.
3. A new guard, `scripts/check-tooling-inventory.py`, **fails closed** when any of those
   figures drifts, so the next plan to add a guard does not silently re-introduce B.

## Invariants

- **`check-workflow-validity.py` keeps exiting 2 when PyYAML is absent.** This plan
  provisions the dependency; it does **not** weaken the guard. A guard that skips silently
  is the Plan 365 defect itself. `tests/planning/test_workflow_validity.py` keeps asserting
  the fail-closed path.
- No closure record is edited. Corrections are forward notes.
- No dated audit snapshot (`docs/architecture/audit/`) is edited.
- Every corrected sentence names the plan or commit that made it false.
- `plans/closure/` and `docs/architecture/audit/` stay byte-identical to their pre-plan
  state for the whole of Plan 372's implementation commit set.
- No new production code, no new capability, no support or advertisement change.
- `specs/support.toml` and `specs/CONFORMANCE.md` are unchanged.
- The new guard **derives** every figure from the tree. It may not carry a hardcoded
  expectation that a human typed; a hardcoded number is the defect being fixed.

## In scope

1. Add PyYAML provisioning to `.github/workflows/ci.yml`.
2. Correct the 17 drifted figures above, each with its responsible plan named.
3. Add `scripts/check-tooling-inventory.py` + `--self-test`.
4. Add the guard to the `AGENTS.md` routine floor and to `ci.yml`.
5. Add a `tests/planning/` companion so the guard's own tests run in the floor.
6. Fix `registry.md:196`, which still describes Plan 369 as `active … WP5–WP6 open`
   while row 188 and `369-status.md` record it as passed.

## Out of scope

- **The `m11-transit-external.yml` failure.** That workflow is `workflow_dispatch`-only yet
  has **100 consecutive failed runs**, all attributed to `push`, dating from before this
  work. It is a distinct, pre-existing GitHub-side condition. Recorded, not fixed.
- **`check-runtime-boundaries.sh`'s inert Tokio manifest rule.** Already carried as an open
  finding in `AGENTS.md` and `369-status.md`; repairing it fails six crates at once and
  needs its own plan-of-record. Untouched here.
- **R1 (`i2pr-api` socket-keyed asymmetry) and R4 (transport grouped `std::net`)** remain
  **open** and must keep being recorded as open.
- Package trust, qualified OS sandboxing, broker, SDK, and UI. Downstream, unregistered.

## Required production changes

None. `.github/workflows/ci.yml` gains a provisioning step; no `crates/**` file changes.

## Work packages

**WP1 — provision PyYAML in CI.** One step in the `quality` matrix job, before the first
guard that needs it, installing into the runner's Python. Both OS legs, because the guard
is platform-neutral and only the image contents differ. Rationale recorded in-line so a
future reader knows the dependency is for `check-workflow-validity.py` specifically.

**WP2 — correct the drifted figures.** In `tooling.md` and `overview.md`. Every correction
names its cause. The `Inventory at a glance` table gets a stated method and a date, so the
next reader can reproduce it rather than trust it.

**WP3 — `check-tooling-inventory.py`.** Derives each published figure from the tree and
compares. Fails closed with a named location and both numbers. Rules:

| Rule | Guards |
|---|---|
| 1 | `AGENTS.md` routine-floor checker-step count |
| 2 | `check-*` files on disk (top level, and with `scripts/interop/`) |
| 3 | every `check-*` script is represented in a `tooling.md` inventory table |
| 4 | `ci.yml` checker-invocation count |
| 5 | distinct checkers named in `ci.yml` |
| 6 | workspace member count and roster in `tooling.md` and `overview.md` |
| 7 | `rust-version` in the root `Cargo.toml` vs the value `tooling.md` publishes |
| 8 | the MSRV toolchain `ci.yml` actually installs |
| 9 | `[[bin]]` count in `fuzz/Cargo.toml` vs the published fuzz-target count |
| 10 | the "zero Rust integration files under `tests/`" claim, inverted to a real count |

Rule 3 is the load-bearing one: it is what makes adding a guard without adding its inventory
row a **failure**, which is precisely the gap Plan 367 found and papered over.

`--self-test` applies each rule's mutation in memory and requires the scan to reject it,
with negative controls so a rule wrong in the strict direction also fails.

**WP4 — wire the guard into the floor, `ci.yml`, and `tests/planning/`.**

**WP5 — fix `registry.md:196`.** Plan 369 is passed; the row must say so and must not
contradict row 188.

## Failure / cancellation / restart semantics

None. The guard is read-only over the working tree; it opens no socket, spawns nothing,
and holds no state. The `ci.yml` change is a provisioning step that fails the job if the
install fails — it must not be `continue-on-error`.

## Compatibility and migration

None. No manifest, dependency, MSRV, feature, or wire change. PyYAML is a **CI-only**
tooling dependency: it is not added to any `Cargo.toml`, not added to `Cargo.lock`, and
introduces no runtime or test dependency into any crate. License MIT, no transitive
package added to the workspace graph. It is deliberately **not** vendored and **not** made
optional — a skipped workflow check is the Plan 365 regression.

## Required tests

- `scripts/check-tooling-inventory.py` — green on this tree.
- `scripts/check-tooling-inventory.py --self-test` — every rule's mutation rejected.
- `tests/planning/test_tooling_inventory.py` — unittest coverage, including that the guard
  fails closed on a drifted count and on an unrepresented checker.
- Existing `tests/planning/test_workflow_validity.py` — unchanged and green; it is the
  regression witness for WP1.

## Exact verification commands

```sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
python3 -m unittest discover -s tests/planning -p 'test_*.py'
python3 scripts/check-workflow-validity.py
python3 scripts/check-tooling-inventory.py
python3 scripts/check-tooling-inventory.py --self-test
bash scripts/check-dependency-direction.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
bash scripts/check-runtime-boundaries.sh
bash scripts/check-console-boundaries.sh
python3 scripts/check-managed-app-process-boundary.py
python3 scripts/check-managed-app-process-boundary.py --self-test
```

## Acceptance criteria

Plan 372 passes only when:

1. `Quality (macos-latest)` is green on the exact-head CI run, and that run is recorded by
   its run id. **Local green is not sufficient for this criterion** — the defect was
   environmental, so only CI can witness the fix.
2. Every one of the 17 figures above matches the tree, and each corrected sentence names
   the plan or commit that made it false.
3. `check-tooling-inventory.py` is green on this tree, and `--self-test` rejects **every**
   rule's mutation.
4. Adding a `check-*` script with no `tooling.md` row makes the guard **fail** — proven by
   injection, not asserted.
5. `plans/closure/` and `docs/architecture/audit/` are byte-identical to their pre-plan
   state, proven by an empty `git diff` over both paths.
6. `check-workflow-validity.py` still exits 2 with PyYAML absent — proven by running it
   with the import blocked, so WP1 is visibly a provisioning change and not a relaxation.
7. R1, R4, the inert Tokio rule, and the `m11-transit-external.yml` 100/100 failure are
   each still recorded as **open** somewhere in `plans/` or `AGENTS.md`.
8. Exact-head routine CI is green, with the run id recorded.

## Stop conditions

Stop and record a classified boundary if any of:

- Provisioning PyYAML in `ci.yml` turns out to require a repository-level dependency
  manifest, or any change to a `Cargo.toml` / `Cargo.lock`.
- Making rule 3 pass requires weakening an existing inventory table rather than completing
  it.
- `Quality (macos-latest)` stays red after provisioning for a reason unrelated to the
  dependency.

## Closure evidence required

Commits; the 17-row before/after table with the method for each; the exact-head CI run id
and per-job conclusions; `--self-test` results; the injection proof for criterion 4; the
`git diff` emptiness proof for `plans/closure/` and `docs/architecture/audit/`; the PyYAML
absent proof for criterion 6; the full routine-floor table with local-vs-CI labels; known
limitations; findings by severity; roadmap disposition; the unblock audit.

## Audit verdict — recorded at registration

**This plan unblocks Plan 367.** Plan 367's acceptance criterion 5 is "exact-head routine
CI is green", and no local action can satisfy it while the macOS leg is red for an
environmental reason. Criterion 5 is therefore reachable only through WP1.

No other registered plan lists Plan 372 as a dependency, and Plan 372 has no hard
dependency of its own. Nothing else moves to `ready` as a result.

The audit also confirms Plan 369 closed nothing else: Plans 370 and 371 are closed, and no
registered plan names Plan 369 or Plan 372 as a hard or interface dependency.