# Plan 096: Plan 095 CI workflow correctness and pre-dispatch closure

## Status and authority

- Status: planned; narrow corrective pass.
- Parent roadmap: Plan 085.
- Immediate predecessor: Plan 095.
- Corrective target: `.github/workflows/ntcp2-interop-host-loopback-development.yml` at or after commit `e50a2f9af8308a2f1fb9805fffc2c45122e65c85`.
- Baseline implementation: Plan 095 implementation commit `5ee412cf0b880bc0f41e39ee9c089dbd64751bb2` plus workflow-context correction `e50a2f9af8308a2f1fb9805fffc2c45122e65c85`.
- Closure target: make the Plan 095 manual GitHub Actions lane execution-correct and statically verifiable before any authoritative live-wire dispatch.
- Next executable action after successful Plan 096 implementation: exactly one manual Plan 095 CI evidence run.
- Plan 088 remains blocked until the Plan 095 instrumented/control evidence pair passes and status reconciliation lands.
- NTCP2 remains experimental and non-advertised.

Active sequence:

```text
Plan 085 -> Plan 086 -> Plan 087 -> Plan 090 -> Plan 091
         -> Plan 092 -> Plan 093 -> Plan 094
         -> Plan 095 (CI lane implementation landed; live run not authoritative)
         -> Plan 096 (workflow correctness / pre-dispatch closure)
         -> one manual Plan 095 evidence run
         -> Plan 088
         -> conditional Plan 079 or Plan 072
```

Plan 096 does **not** replace Plan 095. It repairs the workflow implementation so Plan 095 can execute its already-defined evidence contract. Plan 096 must not produce or fabricate a Plan 095 pass record itself.

## Executive finding

The Plan 095 architecture is sound, but the current workflow contains execution defects that can prevent or invalidate the first authoritative CI evidence run:

1. **i2pr build output path ambiguity / likely failure**
   - The workflow `cd`s into `target/interop/plan095-build`, invokes Cargo without an explicit manifest or target directory, then copies `target/release/i2pr-interop` relative to that build directory.
   - Cargo discovers the repository workspace from an ancestor, so its default target directory is not guaranteed to be the relative path subsequently copied.
   - The workflow must make the Cargo manifest and output path explicit.

2. **sanitized evidence is nested inside disposable run state**
   - The instrumented lane writes sanitized evidence under `target/interop/plan095-instrumented/sanitized`.
   - The later cleanup step removes the parent run root before artifact upload.
   - The same structural problem exists for the control lane.
   - Sanitized evidence must be copied/written outside the disposable run root before destructive cleanup.

3. **control evidence validator uses `os.environ` without importing `os`**
   - The embedded Python validator for the instrumented artifact references `os.environ["INSTRUMENTED_DIR"]` but imports only `json`, `pathlib`, and `sys`.
   - The control job will fail before executing the control attempt.

4. **reference source-tree digest includes Git administrative state**
   - The current `find i2pd -type f | ... sha256sum` input includes `.git/**` files.
   - The resulting digest is partly a clone/worktree metadata digest rather than a canonical digest of the pinned tracked source tree.
   - Plan 095 provenance requires a deterministic source-tree identity derived from tracked source content only.

These are CI workflow correctness defects. They are not evidence of an NTCP2 transport, handshake, framing, data-phase, RouterInfo, observer, or DeliveryStatus failure.

## Objective

Deliver one narrow correction commit that:

1. makes the i2pr build path deterministic and explicit;
2. separates durable sanitized evidence from disposable runtime state;
3. fixes all embedded Python import/runtime errors in the workflow;
4. canonicalizes the pinned i2pd source-tree digest over tracked source files only;
5. strengthens `test_plan095.py` and/or adds `test_plan096.py` so all four failure modes are rejected before workflow dispatch;
6. validates job dependency and fail-closed semantics after the corrections;
7. updates current status authority to name Plan 096 as the pre-dispatch corrective action;
8. commits the workflow correction before any live CI execution;
9. stops after implementation validation and hands off exactly one manual Plan 095 run.

## Non-goals

Plan 096 must not:

- change NTCP2 Noise state-machine logic;
- change NTCP2 frame encryption/decryption;
- change RouterInfo parsing, validation, signing, or identity handling;
- change the Plan 093 bounded data-phase receive oracle unless a regression test proves the workflow cannot invoke it correctly;
- change i2pd observer semantics, generation logic, ring behavior, or exact-target predicates;
- weaken Plan 094/095 provenance requirements;
- alter network ID `99`;
- alter `host-loopback-development` semantics;
- add public I2P connectivity, reseeding, DNS, SAM, I2CP, HTTP, SOCKS, or SSU2;
- introduce Docker, Multipass, namespaces, host networking, firewall changes, or privileged networking;
- add retries to obtain a pass;
- dispatch Plan 095 automatically;
- run Plan 088;
- mark Plan 087, Plan 093, Plan 094, or Plan 095 passed without authoritative CI evidence.

## Mandatory invariants

```text
workflow trigger                    = workflow_dispatch only for live lane
runner                              = github-hosted ubuntu-24.04
topology_kind                       = host-loopback-development
development_only                    = true
release_qualified                   = false
isolation_qualified                 = false
network_id                          = 99
endpoint                            = literal 127.0.0.1:<fresh-port>
reference revision                  = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
instrumented attempt count          = exactly 1 per workflow run
control attempt count               = at most 1 per workflow run
control gate                        = only after validated instrumented pass
raw run state                       = deleted before job completion
sanitized evidence                  = preserved outside raw run root
all pass-eligible digests           = exact nonzero 64-hex
observer drop count                 = 0 for instrumented pass
Plan 093 oracle bounds              = unchanged
Plan 094 exact-target predicates    = unchanged
Plan 088                            = blocked until Plan 095 evidence pair closes
NTCP2                               = experimental/non-advertised
```

## Work package 1: revalidate the current head and defect inventory

Before editing:

1. Record `git rev-parse HEAD`.
2. Verify the head contains both:

```text
5ee412cf0b880bc0f41e39ee9c089dbd64751bb2
e50a2f9af8308a2f1fb9805fffc2c45122e65c85
```

or clean descendants containing equivalent Plan 095 behavior.

3. Inspect:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
tests/integration/ntcp2/harness/test_plan095.py
scripts/check-ntcp2-interoperability.sh
scripts/interop/run-minimal-i2pd-host-loopback-probe.py
tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh
plans/087-status.md
plans/088-status.md
README.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

4. Confirm no successful Plan 095 gate record or authoritative instrumented/control pair has landed after the baseline.
5. Confirm the four defects above still exist before changing them.
6. Do not broaden the pass because unrelated historical Plan 046/048/049 lanes remain unavailable.

Required implementation note:

```text
current_head = <40-hex>
plan095_workflow = implementation-landed-not-yet-authoritatively-run
build_path_defect = present | already-fixed
evidence_cleanup_defect = present | already-fixed
control_import_defect = present | already-fixed
source_tree_digest_defect = present | already-fixed
plan087 = open
plan088 = blocked
```

Acceptance criteria:

- every correction maps to a demonstrated workflow defect;
- no protocol-level file is placed in scope without a specific failing regression demonstrating necessity;
- already-correct behavior is not rewritten gratuitously.

## Work package 2: make the i2pr Cargo build path explicit

Correct the Plan 095 build job so repository discovery and artifact location are deterministic.

Preferred form:

```bash
I2PR_TARGET_DIR="$BUILD_DIR/i2pr-target"

cargo +1.95.0 build \
  --manifest-path "${GITHUB_WORKSPACE}/Cargo.toml" \
  --target-dir "$I2PR_TARGET_DIR" \
  --locked \
  --release \
  --bin i2pr-interop

cp "$I2PR_TARGET_DIR/release/i2pr-interop" \
   "$BUILD_DIR/output/i2pr-interop"
```

Equivalent implementations are acceptable only if both the manifest and target artifact path are explicit and independently checked.

After copy, verify:

```bash
test -f "$BUILD_DIR/output/i2pr-interop"
test -x "$BUILD_DIR/output/i2pr-interop"
test ! -L "$BUILD_DIR/output/i2pr-interop"
```

Compute the binary SHA-256 from that exact downstream artifact, not from a different Cargo output path.

The i2pr build manifest must bind:

```text
source_commit = github.sha
cargo_lock_sha256
rust_toolchain = 1.95.0
i2pr_binary_sha256 = hash(output/i2pr-interop)
build_recipe identity
```

Acceptance criteria:

- no build step depends on Cargo ancestor-workspace target-directory inference;
- the copied binary path is guaranteed to exist before manifest creation;
- the manifest digest matches the exact binary uploaded to downstream jobs;
- a test fails if the workflow returns to relative `cp target/release/...` from `BUILD_DIR` without an explicit target directory.

## Work package 3: separate durable sanitized evidence from disposable run roots

Define disjoint paths for each live job.

Recommended structure:

```text
target/interop/plan095-run/instrumented/        # disposable raw runtime state
target/interop/plan095-run/control/             # disposable raw runtime state

target/interop/plan095-evidence/instrumented/   # sanitized upload-only data
target/interop/plan095-evidence/control/        # sanitized upload-only data
```

Requirements:

1. `--run-root` points only into the disposable run tree.
2. `forward-record.json` may initially exist inside the disposable run tree.
3. The sanitize step reads the raw record and writes a compact allowlisted record into the evidence tree.
4. The cleanup step removes only the disposable run tree.
5. The cleanup step explicitly asserts the evidence tree still exists after deletion.
6. `actions/upload-artifact` reads only from the evidence tree.
7. The evidence tree must not contain raw RouterInfo, identities, keys, raw logs, exchange state, packet captures, or complete protocol transcripts.

Required cleanup sequence:

```text
run attempt
   -> sanitize/validate compact record
   -> fsync/write-complete as practical
   -> verify sanitized file exists
   -> delete disposable run root
   -> verify disposable run root absent
   -> verify sanitized file still exists
   -> upload sanitized artifact
```

Instrumented and control paths must be fully independent.

Acceptance criteria:

- deleting the run root cannot remove the artifact upload source;
- static tests prove the sanitized path is not a child of the disposable run root;
- the job fails if the sanitized record is missing after cleanup;
- `if-no-files-found: error` remains enabled;
- raw runtime directories are never uploaded.

## Work package 4: fix embedded Python runtime correctness

Audit every embedded Python block in the workflow, not only the known control validator.

At minimum, fix the control validation block to include:

```python
import json
import os
import pathlib
import sys
```

Then inspect every heredoc/`python3 -c` block for:

```text
missing imports
undefined variables
wrong environment-variable names
path construction against deleted directories
JSON key assumptions inconsistent with the probe schema
exception paths that accidentally return success
```

Prefer extracting complex validation logic into a small repository Python module only when it reduces duplicated inline logic and improves testability. Do not create a new framework for a handful of checks.

If inline Python remains, add a test that parses/extracts the critical snippets or otherwise validates the expected imports/variables statically.

Acceptance criteria:

- no embedded Python block references an unimported module;
- no validator silently converts malformed/missing evidence into success;
- the control job reaches its actual control-attempt command when supplied a synthetic valid instrumented record;
- malformed synthetic records still fail closed.

## Work package 5: canonicalize the pinned i2pd tracked-source digest

Replace the current filesystem-wide digest:

```bash
find i2pd -type f ...
```

because it includes `.git/**` administrative files.

The digest must cover exactly the pinned tracked source content. Preferred algorithm:

```bash
git -C i2pd ls-files -z \
  | while IFS= read -r -d '' path; do
      printf '%s\0' "$path"
      sha256sum "i2pd/$path" | awk '{printf "%s\0", $1}'
    done \
  | sha256sum \
  | awk '{print $1}'
```

An equivalent canonical algorithm is acceptable if it has these properties:

- includes tracked file path identity;
- includes tracked file content identity;
- excludes `.git` administrative files;
- uses deterministic lexical ordering;
- is insensitive to clone depth, reflogs, index timestamps, FETCH_HEAD, and shallow metadata;
- fails if the checked-out revision is not exactly the pinned revision;
- produces exact lower-case 64-hex SHA-256.

Before digesting, verify:

```bash
test "$(git -C i2pd rev-parse HEAD)" = "$I2PD_REVISION"
test -z "$(git -C i2pd status --porcelain --untracked-files=no)"
```

If the instrumented driver build intentionally patches or copies observer code without modifying the source worktree, keep the pinned source digest pristine and separately bind observer patch/header/source digests in the instrumented build manifest.

Acceptance criteria:

- changing `.git/FETCH_HEAD`, index metadata, or shallow metadata cannot change the source-tree digest;
- changing a tracked source file does change the digest;
- renaming a tracked source file changes the digest;
- the pinned revision mismatch blocks the build before live execution.

## Work package 6: strengthen Plan 095/096 workflow regression tests

Prefer adding:

```text
tests/integration/ntcp2/harness/test_plan096.py
```

and extending `test_plan095.py` only where the invariant naturally belongs to the Plan 095 contract.

Minimum Plan 096 cases:

1. workflow exists;
2. i2pr Cargo invocation contains an explicit repository manifest path;
3. i2pr Cargo invocation contains an explicit target directory or equivalent explicit artifact path contract;
4. workflow does not copy `target/release/i2pr-interop` relative to `BUILD_DIR` without explicit target-dir ownership;
5. built i2pr output is asserted regular/executable before hashing;
6. instrumented raw run root and sanitized evidence root are disjoint;
7. control raw run root and sanitized evidence root are disjoint;
8. sanitized evidence path is not a descendant of either disposable run root;
9. cleanup deletes only disposable run paths;
10. evidence existence is rechecked after cleanup;
11. upload paths point to durable sanitized evidence roots;
12. upload uses `if-no-files-found: error`;
13. control instrumented-evidence validator imports `os` when using `os.environ`;
14. every critical embedded Python environment name is defined at that step;
15. source digest uses `git ls-files` or an equivalent tracked-file enumeration;
16. source digest does not enumerate `.git` administrative files;
17. source revision equality is checked before digest/build;
18. tracked-source digest is deterministic in a synthetic fixture;
19. `.git` metadata changes do not affect the synthetic digest;
20. tracked content changes do affect the synthetic digest;
21. workflow remains `workflow_dispatch` for live execution;
22. live jobs remain `ubuntu-24.04`;
23. live jobs remain loopback-only;
24. network ID remains 99;
25. instrumented attempt count remains one;
26. control remains gated by validated instrumented evidence;
27. control attempt count remains at most one;
28. no automatic retry loop is added;
29. Plan 088 remains absent from the workflow execution path;
30. NTCP2 remains development-only/non-advertised in current authority.

Tests must validate semantics rather than only look for one magic string where practical.

Acceptance criteria:

- each of the four known defects has at least one regression test that fails on `e50a2f9...` behavior and passes after correction;
- Plan 093/094/095 existing tests remain green;
- tests do not require a live router or network access.

## Work package 7: add a lightweight pre-dispatch workflow audit

Add or extend a repository check so obvious workflow execution defects are caught before a manual dispatch.

Preferred ownership:

```text
scripts/check-ntcp2-interoperability.sh
```

or a narrowly named helper invoked by it, such as:

```text
scripts/check-plan095-workflow.sh
```

The audit should verify at minimum:

```text
workflow file exists
manual trigger retained
ubuntu-24.04 retained
build artifact paths explicit
sanitized evidence roots disjoint from run roots
control validator import/variable contract valid
tracked-source digest excludes .git
artifact upload paths are allowlisted
raw run roots are not upload paths
control depends on instrumented success/validation
no retry loop introduced
```

Do not attempt to emulate the entire GitHub Actions expression engine. This is a repository-specific boundary check, not a general workflow linter.

If available in the implementation environment, a standard YAML parser/actionlint-style syntax check may be used as an additional check, but Plan 096 must not add a large new dependency solely for this purpose.

Acceptance criteria:

- the four current defects are statically detectable;
- the checker remains small and specific to the Plan 095 workflow contract;
- existing rootless/Multipass boundary checks remain historical/static only and are not reintroduced as runtime prerequisites.

## Work package 8: preserve fail-closed live-attempt semantics

Review the corrected workflow around `continue-on-error` and shell `|| echo` handling.

The workflow may capture a failed probe long enough to sanitize/upload diagnostics, but it must subsequently fail the job when the attempt did not satisfy the pass predicate.

Required behavior:

```text
instrumented command fails
  -> sanitized diagnostic produced
  -> raw run root deleted
  -> diagnostic artifact uploaded
  -> job concludes failure
  -> control does not start

control command fails
  -> sanitized diagnostic produced
  -> raw run root deleted
  -> diagnostic artifact uploaded
  -> job concludes failure
  -> validate-gate cannot pass
```

Do not let:

```bash
command || echo "failed"
```

become the terminal success state of a live job without a later explicit evidence validator that exits nonzero for nonpassing evidence.

Acceptance criteria:

- synthetic nonpassing evidence causes the live job validation step to exit nonzero;
- diagnostic upload can still run under `if: always()` or equivalent;
- control cannot execute on an instrumented protocol failure, CI blocker, malformed record, or provenance failure;
- validate-gate cannot execute as success on a failed control attempt.

## Work package 9: keep build-artifact and evidence-artifact trust boundaries separate

Downstream jobs must distinguish:

```text
plan095-build              # executables/manifests only
plan095-instrumented-evidence
plan095-control-evidence
plan095-gate
```

Build artifact allowlist:

```text
i2pr-interop
i2pd_ntcp2_interop_driver_instrumented
i2pd_ntcp2_interop_driver_control
build manifests
source digest record
inspect metadata required by existing contract
```

Evidence artifact allowlist:

```text
compact sanitized JSON record
digest companion if used
```

Forbidden from evidence uploads:

```text
raw/
state/
i2pd-data/
exchange/
router.info
identity/key files
pcap/captures
unredacted logs
socket dumps
plaintext/ciphertext transcript material
```

Acceptance criteria:

- artifact directories are disjoint;
- downstream validation checks the expected artifact type/schema before using it;
- no executable is accepted from an evidence artifact;
- no runtime identity material is accepted from a build artifact.

## Work package 10: update current status authority before dispatch

After the Plan 096 correction lands but before a successful Plan 095 live run, current authority should read semantically as:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-implemented-awaiting-authoritative-run
plan_096 = workflow-correctness-completed-pre-dispatch
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

Update only current authority surfaces that presently describe Plan 095 as immediately runnable if that would be inaccurate before the Plan 096 fix:

```text
plans/087-status.md
plans/088-status.md
README.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

Do not create a fake `plans/095-status.md = passed` record.

Acceptance criteria:

- documentation distinguishes `workflow implementation corrected` from `live evidence passed`;
- Plan 088 remains blocked;
- rootless/Multipass remain non-prerequisites for this development-only lane;
- no release/isolation claim is added.

## Work package 11: full pre-dispatch validation

Run at minimum:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan096.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan095.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'

bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh

cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps

git diff --check
```

Also validate the workflow YAML with an existing available parser/linter if one is already present in the environment or repository. Do not add a large dependency solely to satisfy this item.

Acceptance criteria:

- every required check passes from a clean tree;
- no required test is skipped and counted as pass;
- no live i2pr/i2pd attempt occurs during Plan 096 implementation validation;
- the workflow can be parsed as YAML and retains the intended job dependency graph.

## Work package 12: commit before any manual Plan 095 run

The Plan 096 implementation must be committed before dispatch.

Required implementation commit includes only the minimal necessary surfaces, expected to be approximately:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
tests/integration/ntcp2/harness/test_plan096.py
tests/integration/ntcp2/harness/test_plan095.py        # only if needed
scripts/check-ntcp2-interoperability.sh                # or narrow helper
plans/087-status.md                                    # current authority only
plans/088-status.md                                    # current authority only
README.md / AGENTS.md / skill                          # only current-sequence correction
```

Unexpected changes to NTCP2 runtime/protocol crates require explicit justification in the handoff and should normally be rejected from this pass.

After commit:

```text
git status --short = empty
HEAD = exact correction commit
workflow_sha256 = exact nonzero 64-hex
```

Acceptance criteria:

- the subsequent Plan 095 run is bound to a committed clean workflow source;
- no local edits exist at dispatch time;
- the Plan 096 correction commit can be identified unambiguously in the Plan 095 evidence.

## Work package 13: handoff exactly one manual Plan 095 run

Plan 096 stops before executing the live run unless the executing environment explicitly has GitHub Actions dispatch capability and the handoff instruction separately authorizes execution. The default handoff is to the next executor/operator.

The next action is exactly:

```text
manual dispatch of:
.github/workflows/ntcp2-interop-host-loopback-development.yml
at the clean Plan 096 correction commit
```

Expected Plan 095 order remains:

```text
contract
  -> build
  -> forward-instrumented
  -> forward-control only after validated instrumented pass
  -> validate-gate
```

Do not manually trigger multiple runs in parallel.
Do not use rerun-failed-jobs as a substitute for reviewing a failed evidence cycle.
Do not run Plan 088 in the same pass.

Acceptance criteria:

- Plan 096 handoff names the exact commit to dispatch;
- it states that Plan 095 evidence is still pending;
- one workflow run corresponds to one bounded evidence cycle.

## Stop conditions

Stop immediately and do not broaden scope if any of these occur:

### A. Protocol/runtime regression discovered during static/unit validation

If an existing Plan 093/094 protocol test fails because of a genuine runtime defect unrelated to workflow wiring:

```text
stop Plan 096
record exact failing test and owning file
create a separate narrow successor protocol plan
```

Do not mix a transport redesign into the CI workflow correction.

### B. Workflow requires privileged networking to run host loopback

This would contradict the current lane design. Record the exact requirement and stop. Do not silently add namespaces, Docker, Multipass, host networking, or firewall mutation.

### C. Build-driver contract cannot produce separate instrumented/control binaries

Localize the failure to `build-driver.sh` or its manifest contract and create a narrow build-driver successor plan. Do not weaken instrumented/control equivalence.

### D. Sanitized evidence cannot be separated from raw run state without changing probe schema

Prefer a copy/serialization boundary. Do not retain raw state simply to simplify artifact upload.

### E. Source digest cannot be canonicalized without touching the pinned source worktree

Use Git object/tracked-file enumeration instead. Do not hash `.git` metadata as a substitute.

## Explicit closure criteria

Plan 096 is `passed` only when all of the following are true:

- [ ] Current head/defect inventory was recorded.
- [ ] i2pr build uses explicit manifest/artifact ownership.
- [ ] i2pr downstream binary path is verified before hashing/upload.
- [ ] Instrumented raw run root is disjoint from sanitized evidence root.
- [ ] Control raw run root is disjoint from sanitized evidence root.
- [ ] Cleanup cannot delete sanitized evidence.
- [ ] Evidence existence is verified after cleanup.
- [ ] Instrumented artifact upload uses only the durable sanitized root.
- [ ] Control artifact upload uses only the durable sanitized root.
- [ ] Control validator imports `os` or otherwise removes the undefined reference.
- [ ] All embedded Python blocks pass the focused runtime/static checks.
- [ ] i2pd source digest enumerates tracked source content only.
- [ ] `.git` metadata is excluded from the canonical source digest.
- [ ] Pinned i2pd revision equality is verified before digest/build.
- [ ] Tracked source change alters the synthetic digest test.
- [ ] Git metadata change does not alter the synthetic digest test.
- [ ] Plan 096 regression test file exists and passes.
- [ ] Every known defect has a regression test.
- [ ] Existing Plan 093/094/095 tests pass.
- [ ] Static NTCP2 boundary checker passes.
- [ ] Dependency/runtime/vector checks pass.
- [ ] Workspace format/check/test/clippy/doc gates pass.
- [ ] Workflow remains manual `workflow_dispatch` for live execution.
- [ ] Workflow remains `ubuntu-24.04` and host-loopback-development only.
- [ ] No automatic retries were introduced.
- [ ] Instrumented failure still blocks control.
- [ ] Control failure still blocks validate-gate.
- [ ] Raw runtime state is never uploaded.
- [ ] Current status authority names Plan 095 evidence as pending and Plan 088 as blocked.
- [ ] NTCP2 remains experimental/non-advertised.
- [ ] Correction is committed on a clean tree before live dispatch.
- [ ] Final handoff names one exact manual Plan 095 run as the next action.

A passing Plan 096 does **not** imply:

```text
plan_095 = passed
plan_087 = passed
plan_088 = next-executable
```

Those transitions remain owned by the subsequent successful Plan 095 evidence cycle.

## Expected state at Plan 096 completion

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

## Smaller-model execution sequence

Execute strictly in this order:

1. Read Plan 096 completely.
2. Record current HEAD.
3. Confirm Plan 095/`e50a2f9...` ancestry.
4. Read the complete Plan 095 workflow.
5. Confirm the four known defects against current source.
6. Read `test_plan095.py` and current NTCP2 boundary checker.
7. Fix only the i2pr Cargo build/output path.
8. Add/adjust a focused regression for the build path.
9. Move instrumented sanitized output outside the disposable run root.
10. Move control sanitized output outside the disposable run root.
11. Add post-cleanup evidence-existence assertions.
12. Update artifact-upload paths.
13. Add regressions proving sanitized/run-root disjointness.
14. Fix the missing Python import.
15. Audit all other embedded Python snippets for the same class of defect.
16. Add focused embedded-Python contract regressions.
17. Replace filesystem-wide i2pd digesting with tracked-source canonical digesting.
18. Add pinned-revision verification.
19. Add deterministic synthetic digest tests.
20. Add `.git`-metadata-insensitivity test.
21. Add tracked-file-content-change sensitivity test.
22. Review `continue-on-error` and terminal validation semantics.
23. Ensure instrumented failure still produces diagnostics but fails the gate.
24. Ensure control cannot start after nonpassing instrumented evidence.
25. Ensure control failure blocks final gate.
26. Add/complete `test_plan096.py`.
27. Run Plan 096 focused tests.
28. Run Plan 095/094/093 regressions.
29. Run runner/driver/control/Plan 088 regressions.
30. Run NTCP2 static boundary checks.
31. Run workspace Rust gates.
32. Run `git diff --check`.
33. Update current status authority only as needed.
34. Re-run all affected tests/checks.
35. Review diff for accidental protocol/runtime changes.
36. Commit the correction.
37. Confirm clean tree.
38. Record exact correction commit and workflow digest.
39. Stop.
40. Handoff one manual Plan 095 workflow dispatch as the next action.

## Handoff report requirements

The implementing agent must report:

```text
Plan 096 result: passed | blocked
correction commit: <40-hex>
workflow sha256: <64-hex>
i2pr build path correction: <summary>
evidence retention correction: <summary>
embedded Python correction: <summary>
i2pd source digest correction: <summary>
focused tests: <counts/results>
regression tests: <counts/results>
static checks: <results>
Rust gates: <results>
protocol/runtime files changed: none | <explicit justification>
Plan 095 live evidence: not-run
Plan 087: open
Plan 088: blocked
next action: one manual Plan 095 workflow dispatch at <commit>
```

Do not report Plan 095, Plan 087, or forward NTCP2 interoperability as passed merely because Plan 096 implementation validation succeeds.
