# Plan 097: Plan 095 artifact-path ownership and cleanup verification corrective pass

## Status and authority

- Status: planned; narrow corrective pass.
- Parent roadmap: Plan 085.
- Immediate predecessor: Plan 096.
- Execution target: `.github/workflows/ntcp2-interop-host-loopback-development.yml` at or after `8ea673b34250a1cc6e059f6cf32055ffc94bd031`.
- Baseline Plan 096 implementation: `0f480b713d86fd1c44e7b30fa96c977cba392745`.
- Subsequent CI prerequisite corrections already landed:
  - `c560359e8af7f10f0c1fe327a7782f2bab44848a` — install `rustfmt` and `clippy` components in the contract job;
  - `8ea673b34250a1cc6e059f6cf32055ffc94bd031` — install `ripgrep` for the static Multipass boundary check.
- Closure target: remove the remaining deterministic workflow-path and cleanup-verification defects before another authoritative Plan 095 dispatch.
- Next executable action after successful Plan 097 implementation: exactly one manual Plan 095 GitHub Actions dispatch from the clean Plan 097 correction commit.
- Plan 087 remains open until the Plan 095 instrumented/control forward evidence pair passes.
- Plan 088 remains blocked until Plan 095 closes and status authority is reconciled.
- NTCP2 remains experimental and non-advertised.

Active sequence:

```text
Plan 085 -> Plan 086 -> Plan 087 -> Plan 090 -> Plan 091
         -> Plan 092 -> Plan 093 -> Plan 094
         -> Plan 095 (CI lane)
         -> Plan 096 (four workflow corrections landed)
         -> CI prerequisite fixes (rustfmt/clippy, ripgrep)
         -> Plan 097 (artifact ownership + cleanup verification)
         -> one manual Plan 095 evidence run
         -> Plan 088
         -> conditional Plan 079 or Plan 072
```

Plan 097 does **not** replace Plan 095 or Plan 096. It closes two workflow defects that remained after Plan 096 and prevents another manual run from being consumed by predictable CI harness failures.

## Executive finding

The recent Plan 095 attempts have not yet produced negative NTCP2 interoperability evidence. They failed in the pre-live CI apparatus and exposed missing runner prerequisites, which have now been corrected.

The Plan 096 implementation correctly landed its four intended changes:

```text
explicit Cargo manifest/target ownership
sanitized evidence disjoint from disposable run roots
embedded Python import correctness
tracked-source-only i2pd source digest
```

However, the current workflow still has two narrow correctness defects.

### Defect A: producer/consumer artifact path mismatch

The current `build-i2pr-interop` step defines:

```bash
I2PR_TARGET_DIR="$BUILD_DIR/i2pr-target"
```

and correctly builds from an explicit repository manifest and target directory, but then writes the downstream artifact with a relative destination:

```bash
mkdir -p output
cp "$I2PR_TARGET_DIR/release/i2pr-interop" \
   output/i2pr-interop
```

That step does not establish `cd "$BUILD_DIR"` immediately before these relative `output/...` operations. Therefore the artifact producer may write to:

```text
$GITHUB_WORKSPACE/output/i2pr-interop
```

while the following `hash-i2pr-build-manifest` step explicitly does:

```bash
cd "$BUILD_DIR"
sha256sum output/i2pr-interop
```

and therefore consumes:

```text
$BUILD_DIR/output/i2pr-interop
```

The workflow must not depend on implicit current working directory to establish the identity of the binary passed to downstream jobs.

### Defect B: raw run-root deletion is not actually proven

The current instrumented/control cleanup shape is equivalent to:

```bash
find "$PLAN095_RUN_ROOT" -mindepth 1 -delete
test ! -e "$PLAN095_RUN_ROOT" || true
```

The first command removes descendants but normally leaves the root directory. The second command explicitly suppresses a failed absence assertion with `|| true`.

Plan 096's intended contract was stronger: raw run state must be deleted before job completion and the workflow must prove the disposable run root is absent while sanitized evidence survives.

These are CI harness defects, not NTCP2 protocol defects.

## Objective

Deliver one narrow corrective implementation that:

1. establishes one absolute canonical `BUILD_OUTPUT` path for the i2pr artifact;
2. makes every producer, verifier, manifest generator, artifact uploader, and live consumer use that same path;
3. removes all ambiguous relative `output/...` ownership from the Plan 095 i2pr build path;
4. makes raw instrumented/control run-root deletion destructive and verifiable;
5. removes any `|| true` or equivalent suppression from the raw-run-root absence assertion;
6. extends Plan 096/097 regression tests to prove producer/consumer path identity and strict cleanup semantics;
7. extends `scripts/check-plan095-workflow.sh` so both defects are rejected before dispatch;
8. runs all relevant static/unit/workspace validation;
9. commits the correction before any further manual Plan 095 run;
10. stops after implementation validation and hands off exactly one manual Plan 095 dispatch.

## Non-goals

Plan 097 must not:

- change NTCP2 Noise handshake logic;
- change NTCP2 frame encryption/decryption;
- change RouterInfo parsing, signing, verification, or endpoint selection;
- change the Plan 093 multi-frame receive oracle;
- change DeliveryStatus correlation logic;
- change i2pd observer generation/ring semantics;
- weaken exact-target evidence predicates;
- change network ID `99`;
- change `host-loopback-development` semantics;
- add public I2P connectivity, reseed/bootstrap, DNS, SAM, I2CP, HTTP, SOCKS, or SSU2;
- introduce Docker, Multipass execution, namespaces, privileged networking, firewall mutation, or host-network containers;
- add automatic retries;
- dispatch Plan 095 automatically;
- execute Plan 088;
- mark Plan 087, Plan 093, Plan 094, or Plan 095 passed without the authoritative CI evidence pair;
- reinterpret prior CI contract-gate failures as protocol failures.

## Mandatory invariants

```text
workflow trigger                     = workflow_dispatch only for live lane
runner                               = github-hosted ubuntu-24.04
topology_kind                        = host-loopback-development
development_only                     = true
release_qualified                    = false
isolation_qualified                  = false
network_id                           = 99
endpoint                             = literal 127.0.0.1:<fresh-port>
reference revision                   = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
instrumented attempt count           = exactly 1 per workflow run
control attempt count                = at most 1 per workflow run
control gate                         = only after validated instrumented pass
build artifact identity              = one absolute canonical path
build artifact producer              = exact canonical path
build artifact verifier              = exact canonical path
build manifest hash input            = exact canonical path
uploaded build artifact              = exact canonical path tree
live i2pr binary input               = exact downloaded canonical artifact
raw instrumented run root            = absent after cleanup
raw control run root                 = absent after cleanup
sanitized instrumented evidence      = present after raw cleanup
sanitized control evidence           = present after raw cleanup
cleanup assertions                   = fail closed; no suppression
Plan 093 oracle bounds               = unchanged
Plan 094 exact-target predicates     = unchanged
Plan 088                             = blocked until Plan 095 evidence pair closes
NTCP2                                = experimental/non-advertised
```

## Work package 1: freeze and revalidate current repository state

Before editing:

1. Record current `main` HEAD.
2. Verify current head is `8ea673b34250a1cc6e059f6cf32055ffc94bd031` or a clean descendant containing equivalent corrections.
3. Confirm the following commits are ancestors:

```text
0f480b713d86fd1c44e7b30fa96c977cba392745
c560359e8af7f10f0c1fe327a7782f2bab44848a
8ea673b34250a1cc6e059f6cf32055ffc94bd031
```

4. Inspect:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
scripts/check-plan095-workflow.sh
scripts/check-ntcp2-interoperability.sh
tests/integration/ntcp2/harness/test_plan095.py
tests/integration/ntcp2/harness/test_plan096.py
plans/087-status.md
plans/088-status.md
README.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

5. Confirm no successful Plan 095 gate record or authoritative instrumented/control pair has landed after the baseline.
6. Confirm there is still no authoritative `plans/093-status.md` closure record.
7. Confirm Defect A and Defect B are present before correcting them.

Required implementation note:

```text
current_head = <40-hex>
plan096 = passed-pre-dispatch-workflow-correction
contract_prerequisite_rustfmt_clippy = corrected
contract_prerequisite_ripgrep = corrected
artifact_path_ownership_defect = present | already-fixed
raw_run_root_cleanup_defect = present | already-fixed
plan095_authoritative_pair = absent | present
plan087 = open | passed
plan088 = blocked | next-executable
```

Acceptance criteria:

- no already-valid evidence is duplicated;
- no protocol-level source file enters scope without a new direct failing protocol regression;
- Plan 097 remains limited to CI workflow correctness.

## Work package 2: define one canonical absolute build-output root

The build job must define an explicit path used by every i2pr artifact operation.

Preferred shape:

```bash
BUILD_DIR="${GITHUB_WORKSPACE}/target/interop/plan095-build"
BUILD_OUTPUT="$BUILD_DIR/output"
I2PR_TARGET_DIR="$BUILD_DIR/i2pr-target"

mkdir -p "$BUILD_OUTPUT" "$I2PR_TARGET_DIR"
```

The i2pr build step should then use:

```bash
cargo +1.95.0 build \
  --manifest-path "${GITHUB_WORKSPACE}/Cargo.toml" \
  --target-dir "$I2PR_TARGET_DIR" \
  --locked \
  --release \
  --bin i2pr-interop

cp "$I2PR_TARGET_DIR/release/i2pr-interop" \
   "$BUILD_OUTPUT/i2pr-interop"
```

All assertions must operate on the absolute output path:

```bash
test -f "$BUILD_OUTPUT/i2pr-interop"
test -x "$BUILD_OUTPUT/i2pr-interop"
test ! -L "$BUILD_OUTPUT/i2pr-interop"
```

Equivalent implementations are acceptable only if every artifact producer and consumer resolves to the same explicit absolute path without relying on inherited step working directory.

Acceptance criteria:

- no i2pr artifact producer uses relative `output/i2pr-interop` unless the step explicitly and locally proves `pwd == $BUILD_DIR` before the operation;
- preferred implementation uses absolute paths throughout;
- the artifact path is stable regardless of the step's initial working directory;
- the build fails if the expected i2pr binary is absent at the canonical output path.

## Work package 3: bind manifest generation to the exact canonical artifact

The `hash-i2pr-build-manifest` step must hash the exact file produced in Work package 2.

Required behavior:

```bash
actual_i2pr_sha256="$(sha256sum "$BUILD_OUTPUT/i2pr-interop" | awk '{print $1}')"
```

Then write that exact value to:

```text
i2pr-build-manifest.json.i2pr_binary_sha256
```

Before manifest creation:

```bash
test -f "$BUILD_OUTPUT/i2pr-interop"
test -x "$BUILD_OUTPUT/i2pr-interop"
test ! -L "$BUILD_OUTPUT/i2pr-interop"
```

After manifest creation, parse the JSON and verify:

```text
manifest.i2pr_binary_sha256 == sha256(canonical i2pr artifact)
manifest.source_commit == github.sha
manifest.cargo_lock_sha256 == sha256(repo Cargo.lock)
```

Acceptance criteria:

- no second or alternate i2pr binary path may satisfy manifest verification;
- the manifest cannot pass if the uploaded binary differs from the hashed binary;
- a synthetic path-mismatch regression must fail.

## Work package 4: make build artifact upload consume the same canonical tree

`actions/upload-artifact` must upload the exact canonical build-output directory.

Required shape:

```yaml
with:
  name: plan095-build
  path: ${{ github.workspace }}/target/interop/plan095-build/output/
  if-no-files-found: error
```

The prior shell steps must populate exactly that directory.

Before upload, verify at minimum:

```text
i2pr-interop exists/executable/non-symlink
instrumented i2pd driver exists/executable/non-symlink
control i2pd driver exists/executable/non-symlink
all required manifests exist
all required manifest digests validate
```

Acceptance criteria:

- the upload path is not a sibling or alternate output directory;
- there is one authoritative i2pr file inside the uploaded artifact tree;
- no raw run state or identity material is included in the build artifact.

## Work package 5: verify downstream jobs consume exactly the uploaded artifact

Instrumented and control jobs must download the build artifact into one explicit path and invoke:

```text
<download-root>/i2pr-interop
```

The path must be structurally equivalent to the producer path after artifact extraction.

Both live jobs must validate before use:

```bash
test -f "$I2PR_BINARY"
test -x "$I2PR_BINARY"
test ! -L "$I2PR_BINARY"
```

Where practical, recompute SHA-256 and compare against the downloaded `i2pr-build-manifest.json` before starting either router process.

Acceptance criteria:

- downstream jobs cannot silently use a checkout-built or stale alternate i2pr binary;
- manifest/binary digest mismatch blocks live execution;
- instrumented/control use the same i2pr build artifact from the same workflow run.

## Work package 6: make disposable run-root cleanup strict and complete

Replace the current descendant-only deletion plus suppressed absence assertion.

Preferred implementation:

```bash
rm -rf -- "$PLAN095_RUN_ROOT"
test ! -e "$PLAN095_RUN_ROOT"
```

If a safer repository-specific helper already exists, it may be used only if it proves the path is inside the expected workflow workspace before deletion.

Required safety guard before `rm -rf`:

```bash
case "$PLAN095_RUN_ROOT" in
  "${GITHUB_WORKSPACE}/target/interop/plan095-instrumented") ;;
  "${GITHUB_WORKSPACE}/target/interop/plan095-control") ;;
  *) echo "refusing unexpected PLAN095_RUN_ROOT: $PLAN095_RUN_ROOT" >&2; exit 72 ;;
esac
```

Equivalent exact-prefix/realpath guarding is acceptable.

After deletion:

```bash
test ! -e "$PLAN095_RUN_ROOT"
```

No `|| true`, `set +e`, ignored exit code, or equivalent suppression may appear around this assertion.

Acceptance criteria:

- root directory and all descendants are absent after cleanup;
- unexpected cleanup path fails closed before destructive deletion;
- cleanup failure fails the job;
- no raw run root can remain while a job is classified as clean.

## Work package 7: prove sanitized evidence survives strict raw cleanup

After deleting the raw run root, the job must prove the disjoint sanitized evidence remains.

Instrumented:

```bash
test -d "$PLAN095_SANITIZED"
test -f "$PLAN095_SANITIZED/forward-instrumented-sanitized.json"
```

Control:

```bash
test -d "$PLAN095_SANITIZED"
test -f "$PLAN095_SANITIZED/forward-control-sanitized.json"
```

The sanitized paths must remain outside both raw run roots.

Acceptance criteria:

- raw root absent + sanitized record present are both required simultaneously;
- cleanup cannot pass merely because the raw tree was emptied while the root remains;
- evidence upload cannot proceed from a path inside the raw tree.

## Work package 8: add focused Plan 097 regressions

Create:

```text
tests/integration/ntcp2/harness/test_plan097.py
```

or extend `test_plan096.py` only if the new cases remain clearly attributable. Prefer a dedicated Plan 097 file for handoff clarity.

Required cases:

1. Plan 095 workflow exists.
2. Canonical build directory is explicit.
3. Canonical build output directory is explicit.
4. i2pr target directory is explicit.
5. i2pr Cargo manifest path is explicit.
6. i2pr Cargo target directory is explicit.
7. producer copies to the canonical build-output directory.
8. producer does not rely on relative `output/i2pr-interop` from an unspecified CWD.
9. producer verifies exact canonical artifact exists.
10. producer verifies exact canonical artifact is executable.
11. producer verifies exact canonical artifact is not a symlink.
12. manifest hash reads from the same canonical artifact path.
13. manifest `i2pr_binary_sha256` is checked against that artifact.
14. upload-artifact path equals the canonical output tree.
15. instrumented download path is the expected artifact extraction tree.
16. control download path is the expected artifact extraction tree.
17. live jobs verify downloaded i2pr binary before execution.
18. synthetic test: change producer output path but not manifest path -> regression fails.
19. synthetic test: change manifest path but not producer path -> regression fails.
20. synthetic test: change upload path away from canonical output -> regression fails.
21. instrumented cleanup uses strict recursive root removal or equivalent.
22. control cleanup uses strict recursive root removal or equivalent.
23. instrumented cleanup validates the exact expected run root before deletion.
24. control cleanup validates the exact expected run root before deletion.
25. cleanup absence assertion has no `|| true`.
26. cleanup absence assertion has no ignored exit code.
27. instrumented raw root must be absent after cleanup.
28. control raw root must be absent after cleanup.
29. instrumented sanitized evidence must remain after cleanup.
30. control sanitized evidence must remain after cleanup.
31. sanitized evidence remains disjoint from both run roots.
32. `if-no-files-found: error` remains enabled.
33. no retry action/loop is introduced.
34. workflow remains `workflow_dispatch`.
35. live jobs remain `ubuntu-24.04`.
36. network ID remains `99`.
37. topology remains `host-loopback-development`.
38. Plan 088 remains absent from this workflow.
39. NTCP2 remains experimental/non-advertised.
40. status authority still leaves Plan 087 open and Plan 088 blocked before live evidence.

Acceptance criteria:

- at least one Plan 097 regression fails against the `8ea673b...` workflow for Defect A;
- at least one Plan 097 regression fails against the `8ea673b...` workflow for Defect B;
- all Plan 097 regressions pass after correction;
- existing Plan 095/096 regressions remain green.

## Work package 9: strengthen `scripts/check-plan095-workflow.sh`

Extend the pre-dispatch audit to reject both remaining defects.

Required checks:

### Artifact path ownership

The audit must prove, structurally or semantically, that:

```text
producer artifact path
manifest hash input path
verification path
upload tree
```

all resolve to the same canonical build output.

At minimum reject:

```text
mkdir -p output
cp ... output/i2pr-interop
```

when the step does not explicitly establish `cd "$BUILD_DIR"` and the subsequent steps consume `$BUILD_DIR/output`.

Prefer requiring absolute `$BUILD_OUTPUT/...` usage instead of trying to infer step working directories.

### Cleanup strictness

Reject any active cleanup line matching equivalent behavior to:

```text
test ! -e "$PLAN095_RUN_ROOT" || true
```

Reject cleanup that only deletes descendants while retaining the root unless the workflow separately removes the root and proves absence.

Require an explicit post-delete absence assertion.

Acceptance criteria:

- checker fails on `8ea673b...` behavior;
- checker passes after Plan 097 correction;
- checker remains narrow and repository-specific rather than becoming a generic GitHub Actions parser.

## Work package 10: retain fail-closed live-attempt semantics

While correcting paths/cleanup, preserve the Plan 096 behavior:

```text
instrumented command nonzero -> instrumented job nonpassing
instrumented terminal_result != passed -> control forbidden
instrumented cleanup_result != clean -> control forbidden
missing/nonzero provenance failure -> control forbidden
control nonzero -> validate-gate cannot pass
```

Do not reintroduce:

```bash
... || echo "attempt failed"
```

or other masking behavior.

Acceptance criteria:

- attempt exit codes remain authoritative;
- sanitized failure records may still upload for diagnostics, but downstream pass gates cannot treat them as pass evidence;
- exactly one instrumented + at most one control attempt remains enforced.

## Work package 11: status authority update before live dispatch

After the correction lands but before any Plan 095 run, status authority should be equivalent to:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

Update only files that actually carry active execution authority, such as:

```text
plans/087-status.md
plans/088-status.md
README.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

Do not create a passed `plans/093-status.md` yet.

Acceptance criteria:

- one next executable action is named: a single manual Plan 095 dispatch;
- Plan 088 remains blocked;
- no status file claims live evidence exists before it does.

## Work package 12: required validation before commit

Run and report every required command.

Focused Plan 097/096/095 surface:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan097.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan096.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan095.py'
bash scripts/check-plan095-workflow.sh
```

Regression surface:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
```

Static/boundary surface:

```bash
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

Workspace quality gates:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
git diff --check
```

Report each required command as:

```text
executed
passed
failed
skipped
```

A skipped required command does not satisfy Plan 097 closure.

Acceptance criteria:

- all required commands execute and pass;
- no required test is merely skipped;
- clean tree before the corrective commit except intended Plan 097 changes.

## Work package 13: correction commit boundary

The workflow correction must be committed **before** any new Plan 095 manual dispatch.

Record:

```text
plan097_correction_commit = <40-hex>
workflow_sha256 = <64-hex>
check_plan095_workflow_sha256 = <64-hex>
test_plan097_sha256 = <64-hex>
```

Then verify:

```bash
git status --short
git rev-parse HEAD
```

Acceptance criteria:

- live execution never occurs from an uncommitted or dirty correction tree;
- the next Plan 095 run can bind exact source commit provenance to the corrected workflow.

## Work package 14: stop before live execution

Plan 097 implementation stops after the correction commit and validation.

Do **not** dispatch the Plan 095 workflow as part of Plan 097 execution.

The handoff must state exactly:

```text
next_action = one manual Plan 095 dispatch
source_commit = <Plan 097 correction commit>
workflow = .github/workflows/ntcp2-interop-host-loopback-development.yml
attempt_budget = exactly one instrumented + at most one gated control
```

Acceptance criteria:

- Plan 097 cannot mark Plan 095 passed;
- Plan 097 cannot mark Plan 087 passed;
- Plan 097 cannot open Plan 088;
- the next executor has one unambiguous action.

## Manual Plan 095 run after Plan 097

After Plan 097 closes, run exactly one manual workflow dispatch from the exact clean correction commit.

Expected job sequence:

```text
contract
   -> build
   -> forward-instrumented
   -> forward-control only if instrumented evidence validates
   -> validate-gate only if both live jobs satisfy their gates
```

### If contract fails

Classify as a CI contract/bootstrap defect. Do not call it an NTCP2 failure. Do not run control. Do not retry automatically.

### If build fails

Classify as a deterministic build/reference-driver/provenance defect. Do not call it an NTCP2 wire failure unless a router process actually started and emitted wire-stage evidence. Do not run control. Do not retry automatically.

### If instrumented reaches live execution and fails

Use the typed Plan 093/094 terminal reason and retained sanitized record. Do not broaden diagnosis or increase timeouts automatically. Do not run control.

### If instrumented passes and control fails

Classify as control parity/observer divergence unless evidence identifies a narrower cause. Do not close Plan 095/087.

### If both pass

Only then proceed to the existing Plan 095 closure/status reconciliation path:

```text
Plan 093 = passed
Plan 094 = passed-via-Plan095-CI-lane
Plan 095 = passed
Plan 096 = passed
Plan 097 = passed
Plan 087 = passed
Plan 088 = next-executable-not-yet-run
Plan 079 = blocked-pending-Plan088-two-way-pass
Plan 072 = inactive-pending-Plan088-ambiguity
NTCP2 = experimental-non-advertised
```

## Explicit Plan 097 closure criteria

Plan 097 is closed only if **all** are true:

- [ ] current head/ancestry was recorded before editing;
- [ ] no authoritative Plan 095 evidence pair already existed;
- [ ] canonical absolute build output was defined;
- [ ] i2pr producer writes to the canonical output path;
- [ ] i2pr producer does not rely on ambiguous CWD ownership;
- [ ] exact canonical artifact is regular/executable/non-symlink;
- [ ] manifest hashes the exact canonical artifact;
- [ ] manifest digest is checked against the exact artifact;
- [ ] build artifact upload reads the canonical output tree;
- [ ] downstream jobs consume the uploaded canonical artifact;
- [ ] downstream binary digest is checked before live execution where practical;
- [ ] instrumented raw run root is strictly deleted;
- [ ] control raw run root is strictly deleted;
- [ ] cleanup paths are validated before recursive deletion;
- [ ] no cleanup absence assertion is suppressed;
- [ ] instrumented sanitized evidence survives raw cleanup;
- [ ] control sanitized evidence survives raw cleanup;
- [ ] `test_plan097.py` exists and covers both known defects;
- [ ] Plan 097 tests fail against the old defective semantics in synthetic/fixture form;
- [ ] Plan 097 tests pass after correction;
- [ ] `test_plan096.py` remains green;
- [ ] `test_plan095.py` remains green;
- [ ] `scripts/check-plan095-workflow.sh` rejects artifact producer/consumer mismatch;
- [ ] `scripts/check-plan095-workflow.sh` rejects weak cleanup semantics;
- [ ] Plan 094/093/runner/reference regressions pass;
- [ ] static boundary checks pass;
- [ ] Rust workspace quality gates pass;
- [ ] `git diff --check` passes;
- [ ] correction is committed before live execution;
- [ ] status authority names Plan 097 passed and Plan 095 awaiting one authoritative run;
- [ ] Plan 087 remains open;
- [ ] Plan 088 remains blocked;
- [ ] NTCP2 remains experimental/non-advertised;
- [ ] no Plan 095 workflow dispatch occurred during Plan 097 execution.

If any checkbox is false, another Plan 095 live dispatch is not authorized.

## Smaller-model execution sequence

Execute in this order without reordering:

1. Read Plan 097 completely.
2. Record current HEAD.
3. Verify Plan 096 + prerequisite-fix ancestry.
4. Check whether authoritative Plan 095 evidence already exists.
5. Inspect workflow artifact producer path.
6. Inspect manifest consumer path.
7. Inspect upload path.
8. Inspect live download/consumer paths.
9. Prove the current producer/consumer mismatch.
10. Introduce one canonical absolute `BUILD_OUTPUT`.
11. Route producer through `BUILD_OUTPUT`.
12. Route verifier through `BUILD_OUTPUT`.
13. Route manifest hashing through `BUILD_OUTPUT`.
14. Route upload through `BUILD_OUTPUT`.
15. Verify downstream download path matches artifact layout.
16. Add downstream manifest/binary digest check if absent.
17. Inspect instrumented cleanup.
18. Inspect control cleanup.
19. Add exact cleanup path guards.
20. Replace descendant-only deletion with strict root deletion.
21. Remove every suppressed root-absence assertion.
22. Prove sanitized evidence survives cleanup.
23. Add `test_plan097.py`.
24. Add producer/consumer mismatch regression.
25. Add cleanup-suppression regression.
26. Extend `check-plan095-workflow.sh`.
27. Run Plan 097 focused tests/checker.
28. Fix only Plan 097-scoped failures.
29. Run Plan 096/095 regressions.
30. Run Plan 094/093/runner/reference regressions.
31. Run static boundary checks.
32. Run Rust workspace quality gates.
33. Run `git diff --check`.
34. Normalize Plan 087/088/docs/skill status authority if needed.
35. Verify clean intended diff.
36. Commit Plan 097 corrections.
37. Record correction commit/digests.
38. Verify clean worktree.
39. Do not run live CI.
40. Hand off exactly one manual Plan 095 dispatch.

## Executor final report requirements

The Plan 097 implementation report must include:

```text
baseline_head
plan097_correction_commit
files_changed
canonical_build_dir
canonical_build_output
canonical_i2pr_binary_path
manifest_i2pr_hash_input_path
upload_artifact_path
instrumented_download_path
control_download_path
producer_consumer_identity_proof
instrumented_cleanup_path_guard
control_cleanup_path_guard
instrumented_post_cleanup_root_absence
control_post_cleanup_root_absence
instrumented_sanitized_survival
control_sanitized_survival
test_plan097 executed/passed count
test_plan096 executed/passed count
test_plan095 executed/passed count
other regression counts
static check results
cargo quality-gate results
workflow audit result
workflow_sha256
test_plan097_sha256
check_plan095_workflow_sha256
plan087 final token
plan088 final token
NTCP2 support state
next_action = one manual Plan 095 dispatch
```

## Handoff state before execution

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = planned-next-executable-pre-live-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

## Expected handoff state after successful Plan 097 implementation

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
next_action = exactly-one-manual-plan095-dispatch
```
