# Plan 372 — closure

Status: **passed-ci-guard-dependencies-provisioned-and-inventory-drift-now-fails-closed**

Implementation plan:
[`plans/implementation/workspace-foundation/372-provision-ci-guard-dependencies-and-stop-count-drift.md`](../implementation/workspace-foundation/372-provision-ci-guard-dependencies-and-stop-count-drift.md)

Classification: **invariant** (CI provisioning + a drift guard) plus **documentation
corrective**. No production behaviour change.

Subsystem: `workspace-foundation`.

## Commits

| Commit | Content |
|---|---|
| `a6d82299` | Provision PyYAML in `ci.yml`; add `check-tooling-inventory.py` + `--self-test` + `tests/planning/` companion; correct the drifted figures in `tooling.md`/`overview.md`; remove Plan 369's row from the registry's *Blocked work* table; correct four surviving Plan 367-class claims |
| `63658cdc` | `--user` added to the provisioning step after CI run `37582892609` measured PEP 668 |
| `df2e7d5c` | `--break-system-packages` added after CI run `37585213074` measured that `--user` alone is not a PEP 668 exemption in current pip |

Exact-head CI run: **`37585830152` on `df2e7d5c` — success**, all four jobs
(`Quality (ubuntu-latest)`, `Quality (macos-latest)`, `MSRV (Ubuntu)`,
`Dependency policy`).

## What this plan found

Two defects, both reached by execution rather than by reading.

### A — the Plan 365 guard has never passed in CI on macOS

`scripts/check-workflow-validity.py` needs PyYAML and **deliberately exits 2** when the
import fails. That posture is correct and is unchanged by this plan. The defect is that
nothing ever installed the dependency: `ci.yml` had no `pip`/`setup-python` step and the
repository has no dependency manifest of any kind.

`ubuntu-latest` ships PyYAML and passed. `macos-latest` does not. So `Quality
(macos-latest)` was red, and had been red on `main` too:

| Run | Commit | Failing job | Cause |
|---|---|---|---|
| `37519809981` | `main` | `Quality (macos-latest)` | `ModuleNotFoundError: No module named 'yaml'` |
| `37574479845` | `eeabaef1` | `Quality (macos-latest)` | same |
| `37582892609` | `a6d82299` | `Quality (macos-latest)` | `error: externally-managed-environment` (PEP 668) |
| `37585213074` | `63658cdc` | `Quality (macos-latest)` | same — `--user` alone is not a PEP 668 exemption |
| `37585830152` | `df2e7d5c` | — | **success** |

`tests/planning/test_workflow_validity.py` reported **11 errors + 1 failure** on macOS
throughout, and the guard's own behaviour was correct in every case: it refused to report
success on an unchecked tree.

**This is the Plan 365 defect class one level up.** Plan 365's premise was that a floor
step can pass while checking nothing, because nothing parsed YAML. Its fix was a guard.
But no floor step asserted that the *environment* can execute the guard, so the guard
never ran on one of the two platforms CI covers.

### B — the published inventory had drifted, on three surfaces

Plan 367 recomputed `docs/architecture/tooling.md`'s counts and stated a repeatable
method. Plan 369 then landed and they were stale again, and **a third surface Plan 367
never recomputed was already stale**. Every figure below was correct when written.

| Location | Claimed | Actual | Made false by |
|---|---:|---:|---|
| `tooling.md` glance: top-level `scripts/` files | 35 | 54 | Plan 369 (+ earlier plans) |
| `tooling.md` glance: `check-*` on disk, all classes | 36 | 55 | " |
| `tooling.md` glance: checker invocations in `ci.yml` | 24 | 36 | Plans 356–369 |
| `tooling.md` glance: fuzz targets | 24 | 25 | a later plan |
| `tooling.md` glance: workspace members | 20 | 26 | Plans 345, 349, 354, 355, 356, 368, 369 |
| `tooling.md` §`scripts/`: `check-*` files on disk | 49 (56) | 52 (55) | Plan 369 |
| `tooling.md` recompute: floor steps invoking a checker | 40 | 42 | Plans 364/365/369 |
| `tooling.md` recompute: total routine-floor steps | 49 | 52 | Plan 369 |
| `tooling.md` recompute: checkers executed by `ci.yml` | 30 | 34 | Plan 369 |
| `tooling.md` recompute: Method B `Floor: yes` rows | 34 | 44 | Plan 367 under-counted |
| `tooling.md` recompute: Method B `CI: yes` rows | 32 | 37 | " |
| `tooling.md` recompute: Method B checker rows | 44 | 54 | " |
| `tooling.md` §Members header + roster | 20 | 26 | six crates missing, incl. all of Plans 345/354/355/368/369 |
| `tooling.md` MSRV job row | 1.88.0 | 1.89.0 | **Plan 357** |
| `tooling.md` workspace config block | `rust-version = "1.88"` | `1.89` | **Plan 357** |
| `tooling.md` *Distinctive design choices* | MSRV 1.88.0 | 1.89.0 | **Plan 357** |
| `tooling.md` §Zero Rust integration test files | zero `.rs` under `tests/` | 1 tracked | Plan 350 |
| `overview.md:38` | 20 workspace crates | 25 | Plans 345–369 |

**Three of these are worse than a stale digit:**

1. **The MSRV job was documented as running a toolchain the project does not support.**
   Plan 357 raised the floor `1.88 → 1.89` because every published `eggserve-server`
   declares `rust-version = "1.89"`. `AGENTS.md`, the root `Cargo.toml`, `ci.yml`, and
   `docs/architecture/i2pr-daemon.md` all said 1.89; `tooling.md` still described the CI
   job as running 1.88.0. A reader would conclude the MSRV job verifies a toolchain the
   workspace does not claim to support.
2. **"Zero Rust integration test files under `tests/`" is false**, and false in the
   direction that hides a real dependency edge:
   `tests/portable-service-tunnel-consumer/tests/conformance.rs` is tracked, so the
   portable service-tunnel consumer is exercised *outside* its own crate. The claim was
   carrying weight — it is the stated justification for "all verification lives inside
   the crates".
3. **Six workspace members were missing from `tooling.md`'s roster**, including every crate
   of the managed-application runtime and the operator console. A document that lists the
   workspace's crates and omits six of them cannot be used to check the dependency map.

A fourth was found by the new guard rather than by hand: **eleven `check-*` scripts were
in the routine floor with no inventory row at all** — including
`check-console-browser-security.sh`, `check-config-secret-hygiene.sh`,
`check-floodfill-type5-serve.sh`, `check-managed-app-private-client-seams.py`, and both
outproxy wrapper/implementation pairs. Plan 367 had already recorded this class ("two guards
were missing inventory rows entirely") and fixed only the two it happened to name.

### C — four claims that survived Plan 367

| Location | Claim | Correction |
|---|---|---|
| `plans/registry.md` *Blocked work* | Plan 369 `active … WP5–WP6 open` | row removed; it is not blocked work, and row 189 already records it as passed |
| `plans/registry.md:40` | console "not reachable through `i2pr run` while that startup defect stands" | Plan 360 closed that defect |
| `plans/registry.md:315` | same claim, second location | same |
| `plans/subsystems/router-console-roadmap.md:3` | same claim, third location | same |

Plan 367 corrected this exact sentence in the console roadmap's *§351–355* and left the
three instances above. A fourth location, `plans/implementation/floodfill/364-*.md:12`,
was verified as legitimate: it is a plan document quoting its own origin premise.

## Why the corrective is a guard and not another recount

Three separate surfaces in one document published absolute counts of a tree that changes
every time a guard, lane, or crate is added. **A hand recount is the mechanism that
produced the defect**, and this plan demonstrated that twice: it published a Method B
tally from memory that was wrong on all three figures, and the guard caught it.

`scripts/check-tooling-inventory.py` therefore derives every published figure from the
tree and fails closed when one stops matching. It carries no hardcoded expectation, because
a hardcoded number is the defect being fixed.

| Rule | Guards |
|---|---|
| 1 | `AGENTS.md` routine-floor step counts |
| 2 | `check-*` files on disk (top level, and with `scripts/interop/`) |
| 3 | **every `check-*` script has a `tooling.md` inventory row** |
| 4 | `ci.yml` checker invocations |
| 5 | distinct checkers named in `ci.yml` |
| 5b | Method B tallies, recomputed from the document's own tables |
| 6 | workspace member count, `tooling.md` roster, and `overview.md` prose |
| 7 | published `rust-version` vs the manifest |
| 8 | the toolchain the `msrv` job installs, and every documented MSRV job version |
| 9 | `[[bin]]` count in `fuzz/Cargo.toml` vs published fuzz-target claims |
| 10 | the "zero Rust integration files under `tests/`" claim, inverted to a real count |

**Rule 3 is load-bearing.** It is what turns "added a guard, forgot the row" from silent
into a failure, which is precisely the gap Plan 367 found and papered over.

### Derivation decisions, and why

- **Filesystem, not `git ls-files`.** The first draft derived checkers from the git index,
  which cannot see a file the author has not staged — exactly when rule 3 needs to speak.
  A negative control caught it: an untracked injected checker was invisible. Now derived
  from the filesystem.
- **Build artefacts excluded explicitly.** Four `.pyc` files under `scripts/__pycache__/`
  match a `check-*` glob, so an unfiltered `find` reports 58 where the answer is 55. Same
  reason `tests/portable-service-tunnel-consumer/target/` is excluded from the `.rs` count:
  it contains a generated `private.rs`.
- **Fuzz targets counted from the manifest**, not the directory, which also holds a shared
  `support.rs`. That off-by-one is the one `tooling.md` had.
- **`ci.yml` invocations counted as steps**, not commands, so the figure depends on the
  workflow rather than on formatting.
- **`crates/*` counted separately from all members** for `overview.md`, which says "25
  workspace crates plus one non-production launcher tool". Comparing the prose against all
  26 members made a correct sentence look wrong.

## Negative evidence

### 11/11 rules proven load-bearing

Each rule's guard condition was disabled in turn and `--self-test` re-run. A rule that
fails to fire its own control is a rule that does not work.

| Rule disabled | Self-test verdict |
|---|---|
| 1 floor counts | CAUGHT — guard could not run (fail-closed exit 2) |
| 2 check-file count | CAUGHT — `positive control missed: rule2_published_count_wrong` |
| 3 inventory rows | CAUGHT — `positive control fired on the wrong rule: rule3_checker_without_row` |
| 4 ci invocations | CAUGHT — `positive control missed: rule4_ci_invocations_wrong` |
| 5 distinct checkers | CAUGHT — `positive control missed: rule5_distinct_ci_checkers_wrong` |
| 5b Method B | CAUGHT — `positive control missed: rule5b_method_b_tally_wrong` |
| 6 member count | CAUGHT — `positive control missed: rule6_member_count_stale` |
| 7 rust-version | CAUGHT — `positive control missed: rule7_rust_version_stale` |
| 8 MSRV toolchain | CAUGHT — `positive control missed: rule8_msrv_job_documented_stale` |
| 9 fuzz targets | CAUGHT — `positive control missed: rule9_fuzz_count_wrong` |
| 10 zero-tests claim | CAUGHT — `positive control missed: rule10_zero_test_claim_reintroduced` |

**caught=11 escaped=0 skipped=0.** Reproduce with `/tmp/p372-neg.py`.

Note rule 3's verdict text: when rule 3 is disabled, the control still *detects* the
planted checker — but through rule 2, because planting a file changes the count. That is
why `expect_rejected` asserts **its own rule fired**, rather than that any violation
appeared. Plan 369 WP5 found guards whose negative controls passed because an unrelated
rule fired; this guard does not repeat that.

### Three bugs in this plan's own self-test, found by running it

Recorded because they are the class the self-test exists to catch, and they were in the
guard's own controls:

1. **A floor step appended after the closing fence.** The block parser correctly ignores
   it, so the control proved nothing. Now inserted inside the fence.
2. **A member count compared against all members** rather than `crates/*`, so the
   `re.sub` found nothing and the mutation was a no-op.
3. **A string replace against text this plan had already corrected**, so the rule-10
   control silently no-opped. Now injects the claim as text.

A fourth defect was found by inspection rather than by the self-test: rule 10's pattern
assumed `**Zero**` wrapped only the word, but the document wraps the whole clause
(`**Zero Rust integration test files under \`tests/\`.**`), so the rule never fired. The
negative control for it existed and would not have caught this — the pattern and the
mutation were wrong in the same direction. **Recorded as a finding: a guard and its
negative control can share a blind spot.**

### Criterion 6 — the guard is still fail-closed

The PyYAML-absent path was exercised directly with the import blocked:

```sh
$ PYTHONPATH=/tmp/noyaml python3 scripts/check-workflow-validity.py
workflow validity guard could not run: PyYAML is unavailable, so workflow validity
cannot be checked: ... Refusing to report success on an unchecked tree.
exit=2

$ PYTHONPATH=/tmp/noyaml python3 -m unittest discover -s tests/planning -p 'test_workflow_validity.py'
Ran 15 tests ... FAILED (failures=1, errors=11)
```

**WP1 is visibly a provisioning change, not a relaxation.**

### Criterion 4 — an undocumented checker is a failure, by injection

`tests/planning/test_tooling_inventory.py::test_new_checker_without_an_inventory_row`
plants `scripts/check-planted.sh` and asserts rule 3 names it. Rule 3 additionally fired
for real during implementation, on the eleven scripts listed above.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
|---|---|---|
| 1 | `ci.yml` provisions PyYAML on every runner | `a6d82299`; two failed attempts measured, fixed in `63658cdc` + `df2e7d5c` |
| 2 | `Quality (macos-latest)` green on exact-head CI | run **`37585830152`** on `df2e7d5c`, all four jobs success; `PyYAML 6.0.2` imported |
| 3 | The Plan 365 guard is never made to skip | it still exits 2 with the import blocked (above); no change to that script |
| 4 | Every published figure matches the tree | `check-tooling-inventory.py` green; 21-row table above |
| 5 | Each corrected sentence names its cause | every correction names the responsible plan or commit |
| 6 | No closure record or audit snapshot edited | `git diff --name-only a6d82299~1..HEAD -- plans/closure/ docs/architecture/audit/` → **0 files** |
| 7 | `--self-test` rejects every rule's mutation | 11/11 caught, 0 escaped |
| 8 | An undocumented checker fails | injection test + 11 real rule-3 violations found and fixed |
| 9 | The guard is in the floor and in `ci.yml` | `AGENTS.md` routine floor + two `ci.yml` steps |
| 10 | R1, R4, the inert Tokio rule, and the m11 failure stay recorded open | untouched; listed under *Limitations* |
| 11 | Full routine floor green | **54/54 PASS** locally |
| 12 | Exact-head routine CI green | run `37585830152` |

## Verification commands

All run locally on Linux unless labelled otherwise.

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost` | PASS |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | PASS |
| `cargo test --locked --workspace --doc` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — **47 tests** (was 30; Plan 372 adds 17) |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-tooling-inventory.py --self-test` | PASS |
| `python3 scripts/check-workflow-validity.py` | PASS |
| `bash scripts/check-dependency-direction.sh` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-adr-number-uniqueness.py` | PASS |
| 30 further boundary/evidence checkers | PASS (see floor log) |
| `cargo deny check advisories bans sources` | PASS |
| **Full `AGENTS.md` routine floor** | **54/54 PASS** |
| **CI run `37585830152` on `df2e7d5c`** | **success** (CI) |

Floor log: `/tmp/floor372.log`. Floor runner: `/tmp/p372-floor.sh`.

### The floor gained two entries

`python3 scripts/check-tooling-inventory.py` in the `AGENTS.md` routine floor, and two
`ci.yml` steps (the guard and its `--self-test`). The floor is now **52 steps**, 42 of
which invoke a checker. `check-tooling-inventory.py` enforces that figure, so the two
cannot disagree.

## Dependency review

**No new dependency in the workspace graph.** PyYAML is a **CI-only tooling dependency**,
MIT licensed, pure Python, no compiled extension:

- not added to any `Cargo.toml`;
- not added to `Cargo.lock`;
- not vendored into the repository;
- no transitive package enters the Rust graph, so `cargo deny` is unaffected and its
  advisories/bans/licenses result is unchanged.

It is deliberately **not** made optional. A skipped workflow check is the Plan 365
regression, and the guard's own error text says so: *"Install PyYAML or add it
deliberately; do not make this guard skip silently."*

`--break-system-packages` is used on the CI runner and the reasoning is recorded inline in
`ci.yml`: the earlier preference for `--user` alone was wrong (measured on two runs), and
the remaining objection — that it can corrupt a Homebrew installation — applies to a
developer's machine, not to an ephemeral runner installing a pure-Python leaf.

## Findings by severity

**High — the Plan 365 workflow-validity guard had never executed on macOS in CI.** A
security-adjacent guard was inert on half the CI matrix, and its own test suite had been
reporting 11 errors there for as long as the guard existed. Nothing in the floor asserted
that a guard's *environment* can run it. Fixed by provisioning; recorded because the
pattern generalises: a guard's dependency is part of its contract and belongs in the floor.

**Medium — the workspace roster omitted six crates**, including every member of the
managed-application runtime and the operator console. Any reader using `tooling.md` to
check the dependency map was working from a list missing a third of the graph. Now
derived and enforced.

**Medium — the MSRV CI job was documented at a retired toolchain.** A reader would
conclude the MSRV job verifies 1.88.0 while the workspace requires 1.89. Now derived from
the manifest and from `ci.yml`'s own `msrv` job.

**Medium — "zero Rust integration test files under `tests/`" was false**, in the direction
that hides a dependency edge. Now an inverted, derived count.

**Low — eleven floor checkers had no inventory row.** Documentation completeness only;
no enforcement gap, since each is in the floor and CI where it belongs.

**Low — this plan's own Method B tallies were published from memory and were wrong** on
all three figures. This is the mechanism the guard now exists to catch, and it is recorded
rather than quietly corrected, because it is the evidence for rule 5b existing.

**Informational — `.github/workflows/m11-transit-external.yml` has failed 100 of its last
100 runs**, all attributed to `push`, predating this work. It is `workflow_dispatch`-only
and GitHub reports *"This run likely failed because of a workflow file issue."* Its
structure, trigger set, and input-description lengths were all inspected and are valid;
the Plan 365 guard passes it. Recorded as out of scope.

## Limitations

1. **`m11-transit-external.yml` remains broken** — 100/100 failures, unexplained, and
   pre-existing. Out of scope here; needs its own plan-of-record.
2. **`check-runtime-boundaries.sh`'s Tokio manifest rule remains inert.** It greps
   `^(tokio|tokio-util)[[:space:]]*=`, which cannot match this repo's
   `tokio.workspace = true` style. Left unmodified: the rule is to be fixed, not relaxed,
   and repairing it fails six crates at once. Needs its own plan-of-record.
3. **R1** (`i2pr-api` socket-keyed asymmetry) and **R4** (transport crates' grouped
   `std::net`) remain **open** and are still recorded as open in `AGENTS.md` and
   `362-status.md`. Neither is a documentation defect and neither is touched here.
4. **The guard reads documents, not meaning.** It proves a published number matches the
   tree and that a claim of a specific shape is absent. It cannot prove prose is
   *accurate* — only that the specific counts and claims it knows about are not stale.
   A newly invented false claim in another shape is outside its reach.
5. **A rule and its negative control can share a blind spot** — demonstrated by rule 10's
   pattern and its mutation both being wrong in the same direction. The self-test reduces
   this; it does not eliminate it.
6. **`tooling.md`'s Method A/B duality is retained**, since the gap lists below the
   recompute table are written in Method B convention. Two conventions remain; both are
   now derived and enforced, so neither can drift independently.

## Unblock audit

Required at every closure. Performed against `plans/registry.md` blocked work and the
affected roadmap dependency graphs.

**This plan unblocks Plan 367.** Plan 367's acceptance criterion 5 is "exact-head routine
CI is green", and no local action could satisfy it while the macOS leg was red for an
environmental reason. Plan 367's implementation landed at `2f82c799` and its criteria 1–4
were re-verified green on this tree; only criterion 5 was blocked. **Plan 367 is therefore
ready to close and is closed by `plans/closure/workspace-foundation/367-status.md`.**

Every other blocked plan keeps an unrelated blocker, none of which Plan 372 touches:

| Plan | Remaining blocker | Touched by 372? |
|---|---|---|
| 348 | 347 alone | no |
| 347 | Java bandwidth tier (ADR 0030) — stopped at a classified boundary | no |
| 326 | 347 | no |
| 327 | an outproxy that actually answers; no Java/i2pd outproxy exercised | no |
| 325 | no qualified maintained Red25519 provider | no |
| 328 | superseded forward by 348 | no |
| 278 | reference client rejects the controlled RouterInfo — budget spent | no |
| 279 | hard dep Plan 278, still unmet | no |
| 306 | truthful bandwidth-class design | no |
| 308 | controlled HTTP topology + three family captures | no |
| 317 | qualification-only owner attempt; replaced by Plan 318 | no |

No state transition to `ready` is warranted for any of them, and no corrective needs
registering for an unblock.

## Roadmap disposition

`plans/subsystems/workspace-foundation-roadmap.md` gains a Plan 372 row. Its milestone
status summary already records that the lane fixes tooling and one product-path defect
without reopening Milestones 1 or 2; that remains true.

The **package-trust milestone** in the managed-native-app-runtime roadmap is untouched.
It requires its own plan-of-record, as recorded.

## Handoff

The next plan that adds a checker, a workspace crate, a floor step, or a CI step will
**fail `check-tooling-inventory.py`** until `tooling.md` is updated. That is the intended
behaviour, and rule 3's error message says which files are missing their row.

If a count is legitimately excluded (for example an opt-in lane runner), the fix is to
exclude it in the guard's derivation with a comment naming why — **not** to delete the
rule or relax it.