# Plan 100: Plan 099 exit-gate cleanup and router-buildout handoff

## Status and authority

- Status: planned; active one-time closure/cleanup pass.
- Date: 2026-08-11.
- Baseline: `e7cad5f213f896a5545e0df1993a24a7d7c67379` or a clean descendant that has not materially changed the Plan 099 development-interoperability architecture.
- Parent authority: Plan 099 and ADR 0023.
- Primary reference: pinned i2pd 2.60.0 at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Development topology: `host-loopback-development`, literal `127.0.0.1`, network ID `99`.
- NTCP2 remains experimental, non-advertised, and disabled in normal daemon operation.
- Plan 100 is a **one-time exit-readiness cleanup**, not another interoperability framework or evidence roadmap.
- Plan 100 is the only allowed follow-up to the Plan 099 implementation defects identified at the baseline above. It must not create a Plan 101 NTCP2 interoperability sequence.

Plan 099 correctly changed the project direction by deleting the historical plan-specific Python apparatus, collapsing the live development lane to one GitHub Actions job, and authorizing production router/NetDB work independently of release-grade transport qualification. However, Plan 099 has not yet completed its own Phase F/G exit because the compact exit gate and reference-build proof contain several deterministic defects that can prevent or misclassify the final run.

Plan 100 exists only to remove those defects, execute the one remaining bounded development smoke, record one terminal development result, and hand the repository to actual router construction.

## Current repository state

The Plan 099 cleanup materially succeeded:

```text
pre_plan099_tracked_python_loc = 50,260
current_tracked_python_loc     = ~14,356
pre_plan099_tracked_rust_loc   = 35,224
current_tracked_rust_loc       = ~37,599
historical_plan_python_matrix  = removed
plan095_multi_job_artifacts    = removed
active_live_ci_jobs            = 1
normal_daemon_runtime          = RuntimeNotImplemented
ntcp2                          = experimental-non-advertised
```

The current one-job workflow already implements the desired high-level shape:

```text
checkout
  -> build pinned i2pd instrumented + control locally
  -> build i2pr-interop locally
  -> focused functional checks
  -> forward instrumented
  -> forward control when forward instrumented passes
  -> reverse instrumented when forward control passes
  -> reverse control when reverse instrumented passes
  -> compact sanitized result
  -> delete raw run state
```

No binary artifact transfer between jobs is required or allowed.

The latest reference-driver correction also fixed a substantive issue: the instrumented driver now links against an i2pd library compiled from the patched source tree while the control driver links against pristine archives. The remaining defects are closure/readiness defects around this simpler architecture.

## Problem statement

The current baseline has five narrow classes of remaining work.

### D1 — exit-gate workflow/module API mismatch

The workflow imports symbols from `plan099_exit_gate.py` that the module does not define, including names equivalent to:

```text
ATTEMPT_SLOTS
ENVIRONMENT_OR_BUILD_BLOCKED
PASSED_CLEANUP_RESULT
PASSED_TERMINAL_RESULT
```

The workflow also invokes `build_summary()` with a keyword named `ntcp2`, while the module expects `ntcp2_status`.

As committed, the always-run summary step can fail independently of the actual live probe results and therefore cannot reliably retain the final development result.

### D2 — exit classification cannot represent the workflow's sequential gating

The current workflow intentionally stops later attempts when an earlier prerequisite fails. For example, a real forward post-TCP wire failure prevents forward control and both reverse attempts.

The current classifier labels a forward defect only when the forward pair fails **and the reverse pair passes**. That state cannot occur under the workflow's dependency ordering. A genuine forward post-TCP NTCP2 defect can therefore collapse into an environment/build classification merely because the intentionally skipped reverse attempts are absent.

This violates Plan 099's central bounded-failure rule: authentic post-TCP protocol evidence must be distinguished from pre-TCP environment/harness failure.

### D3 — object-level observer proof is not fail-closed in CI

`build-driver.sh` currently invokes proof pipelines using `rtk rg` even though the selected GitHub Actions job installs ordinary `ripgrep` (`rg`) and does not establish `rtk` as a dependency.

The proof pipelines also contain suppression forms such as `|| true` / fallback text. Therefore an unavailable proof command or a missing observer reference can be represented as harmless output rather than a failed build.

The required invariant is much simpler:

```text
pristine observer reference count     == 0
instrumented observer reference count  > 0
```

The build must fail when either condition is false.

### D4 — source-tree digest fallback still has two encodings

The Python source-tree digest branch aggregates raw SHA-256 bytes for child files. The shell fallback aggregates ASCII hexadecimal digest text. These are different algorithms.

Plan 099 explicitly preferred deletion of redundant provenance machinery over maintaining several implementations of the same identity algorithm.

The active Plan 099 lane already requires Python 3. Therefore the preferred correction is to retain one implementation and remove the divergent fallback rather than build a new cross-language digest abstraction.

### D5 — active status authority still describes the superseded Plan 095 -> Plan 088 sequence

Several status documents still say:

```text
Plan 095 = next executable
Plan 087 = open pending Plan 095 pair
Plan 088 = blocked pending Plan 095 closure
```

That no longer matches the Plan 099 architecture. The single Plan 099 workflow now owns both directions and both control comparisons in one run.

The stale status graph creates exactly the planning/artifact churn Plan 099 was intended to stop.

## Objective

Plan 100 must produce exactly this transition:

```text
current Plan 099 implementation
        |
        v
repair exit-gate/runtime-proof defects D1-D4
        |
        v
focused local validation only
        |
        v
commit clean exit-ready candidate
        |
        v
ONE manual Plan 099 single-job dispatch
        |
        +--------------------+---------------------+
        |                    |                     |
        v                    v                     v
      passed       protocol-defect-localized   environment-or-harness-blocked
        |                    |                     |
        +--------------------+---------------------+
                             |
                             v
             reconcile active status once
                             |
                             v
              END NTCP2 planning sequence
                             |
                             v
          production router + NetDB roadmap
```

Plan 100 must not make successful independent NTCP2 interoperability a prerequisite for production daemon composition or local/offline NetDB implementation.

## Hard scope lock

Plan 100 may modify only the smallest surfaces necessary to close the exit:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
tests/integration/ntcp2/harness/plan099_exit_gate.py
or the existing functional module that absorbs that logic
tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py
tests/integration/ntcp2/harness/test_i2pd_direct_driver.py
tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh
scripts/check-ntcp2-interoperability.sh only when needed for durable invariants
plans/099-status.md
plans/087-status.md
plans/088-status.md
plans/079-repeated-i2pd-development-validation-and-continuation-decision.md
README/AGENTS/protocol-support only if they contain active instructions that contradict the final authority
```

No production Rust protocol behavior should change during the pre-live cleanup.

A Rust NTCP2 behavior change is allowed only after the final live dispatch reaches authentic TCP and localizes one reproducible i2pr-owned wire defect, and then only under the bounded correction rule in this plan.

## Non-goals

Plan 100 does not:

- create a new interoperability topology;
- create a new evidence tier;
- create another candidate/certificate/provenance hierarchy;
- restore deleted Plan 052-098 Python machinery;
- add a new `test_plan100.py` plan-text test suite;
- add a new plan-specific runner;
- add retries;
- reintroduce cross-job binary artifacts;
- use rootless namespaces, Multipass, Docker, QEMU, or a VM;
- use the public I2P network;
- reseed or bootstrap NetDB;
- enable SAM/I2CP/HTTP/SOCKS/SSU2;
- enable NTCP2 in the normal daemon;
- advertise NTCP2 in production RouterInfo;
- run Java I2P;
- start Plan 079 repeated validation;
- redesign Noise XK, frame crypto, or the data-phase state machine before authentic wire evidence requires it;
- add Plan 101 for another NTCP2 cleanup pass.

## Work package 1 — establish a truthful active baseline

Before modifying implementation:

1. confirm HEAD is the expected Plan 099 descendant;
2. confirm the workflow is one `workflow_dispatch` job on `ubuntu-24.04`;
3. confirm there is no binary upload/download between jobs;
4. confirm the pinned i2pd revision is exact;
5. confirm NTCP2 remains experimental/non-advertised and the normal daemon remains disabled;
6. record current tracked Rust/Python LOC only for comparison, not as a new permanent evidence artifact;
7. confirm `i2pr-daemon run` still returns `RuntimeNotImplemented` for real execution, establishing why router composition is the next product priority.

Do not add new status schemas or baseline JSON files.

## Work package 2 — repair the compact exit-gate API

### 2.1 One source of truth

The workflow and Python module must use one exact API.

Preferred implementation:

- define only the constants/functions actually needed;
- remove unused imports from the workflow;
- align the `build_summary()` keyword contract exactly;
- add one focused functional round-trip test that calls the same API shape the workflow calls.

Do not solve an import mismatch by adding aliases for every historical name when the name is no longer useful.

### 2.2 Prefer generic functional ownership

`plan099_exit_gate.py` is permitted to be removed or renamed/folded into an existing functional interop module if that makes ownership clearer.

Preferred long-term naming is behavior-oriented, not plan-number-oriented. Examples:

```text
development_interop_result.py
minimal_i2pd_probe.py
```

Do not create an additional file merely to rename it. Move the small logic only if it reduces active plan-specific surface.

### 2.3 Minimal terminal vocabulary

Use the Plan 099 closure vocabulary:

```text
passed
protocol-defect-localized
environment-or-harness-blocked
```

Do not retain a four/five-state plan-specific decision vocabulary unless an existing consumer genuinely requires it.

The terminal result is development-only and must also carry:

```text
development_only = true
release_qualified = false
isolation_qualified = false
ntcp2 = experimental-non-advertised
```

## Work package 3 — make classification stage-aware and compatible with sequential execution

Classification must inspect the attempts that actually ran, not require later intentionally skipped attempts to produce impossible evidence.

### 3.1 Pass

Return `passed` only when all four attempts are present and pass cleanly:

```text
forward instrumented = passed + clean
forward control      = passed + clean
reverse instrumented = passed + clean
reverse control      = passed + clean
```

### 3.2 Protocol defect

Return `protocol-defect-localized` when any executed primary direction has authentic current-run protocol evidence at or after real TCP and then fails before the required correlated DeliveryStatus pass.

Examples include an authentic record whose highest stage proves:

```text
tcp_connected
or later NTCP2 establishment/data-phase state
```

with a nonpassing terminal outcome.

A skipped downstream attempt must not erase that classification.

The compact result must preserve the exact highest authentic stage and the typed reason already produced by the underlying functional probe. Do not invent another large reason vocabulary.

### 3.3 Environment/harness blocker

Return `environment-or-harness-blocked` only when the earliest nonpassing path occurs before authentic TCP/protocol evidence, such as:

```text
build failure
missing executable
reference startup failure
state preparation failure
workflow/API error
loopback placement failure before TCP
```

### 3.4 Sequential skip semantics

An attempt skipped because its prerequisite failed must be represented explicitly as `not-run` / `skipped-after-prerequisite`, not silently reclassified as an environment failure.

The classifier must not require reverse evidence to classify a forward protocol defect.

### 3.5 Tests

Focused tests must cover at least:

1. all four pass -> `passed`;
2. forward pre-TCP failure, later attempts skipped -> `environment-or-harness-blocked`;
3. forward post-TCP failure, later attempts skipped -> `protocol-defect-localized`;
4. forward pair passes, reverse pre-TCP failure -> `environment-or-harness-blocked`;
5. forward pair passes, reverse post-TCP failure -> `protocol-defect-localized`;
6. malformed/unknown record -> fail closed;
7. skipped attempts cannot turn a real protocol defect into environment blocked;
8. summary digest validation round trip.

Put these tests in the existing functional test owner. Do not create `test_plan100.py`.

## Work package 4 — make the i2pd instrumentation proof real and portable

### 4.1 Remove undeclared command dependency

Replace `rtk rg` with a command guaranteed by the workflow's declared dependencies, preferably plain `rg` or standard `grep`.

Do not add an `rtk` installation solely to preserve the current command text.

### 4.2 Hard assertions

After both library builds:

```text
pristine_refs=$(nm -C pristine/libi2pd.a | rg -c 'i2pr::i2pdinterop::Observe' || true)
instrumented_refs=$(nm -C instrumented/libi2pd.a | rg -c 'i2pr::i2pdinterop::Observe' || true)
```

The exact shell may differ, but require:

```text
pristine_refs == 0
instrumented_refs > 0
```

Fail with a short diagnostic if either invariant is violated.

The proof file may still be emitted for inspection, but writing the file is not the proof. The build's success/failure condition is the proof gate.

### 4.3 Binary-level sanity

If inexpensive, also assert:

```text
instrumented binary observer refs > 0
control binary observer refs == 0
```

Do not add objdump/disassembly parsing unless `nm` is insufficient.

### 4.4 Tests

Existing driver tests should fail if:

- instrumented/pristine archive variables collapse to one path;
- the observer macro is not present in the instrumented library build;
- the hard reference-count assertion is removed;
- `rtk` is reintroduced as an undeclared dependency.

## Work package 5 — delete the divergent source-tree digest fallback

Preferred correction:

1. require Python 3 in `build-driver.sh` for the tracked-tree digest;
2. retain one canonical implementation;
3. delete the shell fallback that produces a different byte encoding;
4. fail with a short dependency message if Python 3 is unavailable.

Do not add another script/module just for digest normalization.

If the development smoke no longer consumes the tree digest for a meaningful decision, deletion of the field and its supporting code is preferable to maintaining it.

The minimum reference identity required for this development smoke is:

```text
exact pinned git revision
clean tracked worktree before build
actual instrumented binary SHA-256
actual control binary SHA-256
```

## Work package 6 — keep the workflow one-job and remove obsolete semantics

The Plan 099 one-job design is correct and must remain.

Required invariants:

```text
trigger                = workflow_dispatch only
runner                 = ubuntu-24.04
jobs                   = one live/build job
binary artifact relay  = none
reference revision     = exact pinned i2pd revision
endpoint               = 127.0.0.1 only
network id             = 99
public I2P bootstrap   = forbidden
retries                = none
raw state upload       = forbidden
```

The job may upload exactly one compact sanitized result after raw run directories are deleted.

Do not reintroduce separate contract/build/control/gate jobs.

## Work package 7 — focused local validation before the final run

Run only the surfaces changed by this cleanup plus the normal workspace baseline.

Required:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
git diff --check
```

Run Clippy/rustdoc only if touched Rust surfaces justify them. Do not restore the deleted historical Python matrix.

Before the live dispatch, perform one static reread of the workflow and verify:

- every imported Python name exists;
- every keyword passed to the summary builder exists in its signature;
- the summary step can be exercised locally with synthetic functional records;
- the observer proof uses declared commands;
- no suppressed assertion can convert a missing instrumented observer into success.

## Work package 8 — commit the exit-ready candidate

All code/test/status corrections must be committed before the live run.

Record the exact commit SHA in `plans/099-status.md` as:

```text
plan_100 = exit-ready-awaiting-one-manual-run
plan_099 = implementation-landed-exit-run-pending
```

The working tree must be clean.

Do not modify code during the live run.

## Work package 9 — execute exactly one authoritative Plan 099 run

Manually dispatch:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
```

against the clean exit-ready commit.

No automatic retries.

No second concurrent run.

No workflow architecture change during the attempt.

### Result branch A — passed

If all four attempts pass:

```text
independent_ntcp2_forward   = passed
independent_ntcp2_reverse   = passed
observer_control_neutrality = passed
development_interop         = passed
```

Record the compact summary digest/run identifier and close Plan 100.

Do not immediately start Plan 079. It remains deferred to the pre-normal-activation/public-network integration checkpoint.

### Result branch B — authentic protocol defect

If an attempt reaches authentic TCP and fails during NTCP2 establishment/data phase:

1. preserve the compact sanitized result;
2. identify the highest authentic stage and exact existing reason;
3. reproduce **once** from fresh state with the same code/timeouts/reference revision;
4. if the same failure recurs and ownership is clearly i2pr, permit one narrow Rust correction to the owning NTCP2 implementation/runtime;
5. run focused Rust/interop tests;
6. commit that correction;
7. execute one replacement run of the affected Plan 099 matrix.

If it still fails, record:

```text
development_interop = protocol-defect-localized
exact_wire_stage = <real stage>
exact_question = <one concise protocol question>
```

Then stop NTCP2 corrective planning and proceed to router construction.

Do not create Plan 101.

### Result branch C — environment/harness blocker

If the attempt does not reach authentic TCP:

1. preserve the compact result;
2. identify the exact command/path/environment failure;
3. permit at most one direct narrow correction without a new plan;
4. commit it;
5. dispatch one replacement run.

If it remains blocked, record:

```text
development_interop = environment-or-harness-blocked
```

and proceed to router construction with NTCP2 disabled/non-advertised.

Do not create another execution lane.

## Work package 10 — collapse stale planning authority

After the final Plan 100 outcome, update planning status once.

### 10.1 Plan 099 status becomes the current development-interoperability record

`plans/099-status.md` must carry exactly the active outcome and the retained compact result identity.

It must not say Plan 095 remains the next executable plan.

### 10.2 Plans 087/088 become historical execution authority

Add an active correction at the top of `plans/087-status.md` and `plans/088-status.md` stating that Plan 099/100 supersede their Plan 095 sequencing for development execution.

Preserve historical evidence below; do not rewrite old run descriptions.

The current active development result comes from the Plan 099 one-job forward/reverse matrix.

### 10.3 Plan 079 remains deferred

Amend Plan 079's active status to:

```text
deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
```

It is not a prerequisite for:

- daemon composition;
- local RouterInfo publication architecture;
- local NetDB storage;
- deterministic NetDB lookup/store state machines;
- persistence/restart revalidation;
- offline SU3 ingestion.

### 10.4 Do not cascade plan tokens through the repository

Update README/AGENTS/protocol-support only where they contain active instructions that would otherwise send the next implementation agent back to Plan 095/088.

Do not rewrite historical plan prose merely to eliminate old plan numbers.

## Work package 11 — mandatory router-buildout handoff

Plan 100 closes only by naming the next product-development scope.

The next substantial roadmap must address actual Rust router construction, beginning with:

```text
production i2pr-daemon composition
    -> persistent router identity load
    -> Tokio Supervisor/service graph
    -> readiness / shutdown ownership
    -> local RouterInfo publication owner
    -> validated NetDB storage foundation
    -> expiry / replacement / quotas
    -> persistence + restart revalidation
    -> deterministic DatabaseStore / Lookup / SearchReply state machines
    -> offline/local SU3 ingestion
```

If independent NTCP2 passes, that roadmap may include a strictly experimental/default-disabled transport service composition seam. Production RouterInfo advertisement remains false until the later activation checkpoint.

If NTCP2 is localized/blocked, the roadmap proceeds without live peer exchange and leaves external NetDB-over-NTCP2 integration blocked.

## Explicit acceptance criteria

Plan 100 is complete only when all applicable criteria below are satisfied.

### Exit-gate correctness

1. Every Python name imported by the workflow exists at runtime.
2. Every workflow keyword passed to the summary builder matches its function signature.
3. The compact result vocabulary is exactly `passed`, `protocol-defect-localized`, or `environment-or-harness-blocked`.
4. A four-attempt clean pass classifies as `passed`.
5. A forward post-TCP failure with downstream attempts skipped classifies as `protocol-defect-localized`.
6. A reverse post-TCP failure classifies as `protocol-defect-localized`.
7. A pre-TCP preparation/build failure classifies as `environment-or-harness-blocked`.
8. Skipped downstream attempts are represented as skipped/not-run and do not override the highest authentic executed stage.
9. Malformed/unknown records fail closed.
10. Summary digest validation round-trips.
11. No new large reason-code vocabulary is introduced.
12. No `test_plan100.py` exists.

### Reference build correctness

13. Instrumented i2pd libraries are compiled from the patched tree with observer call sites active.
14. Control libraries are compiled from the pristine pinned tree.
15. Instrumented and control binaries link to their respective library sets.
16. `rtk` is not a required command in the build/proof path.
17. Pristine observer-reference count is asserted exactly zero.
18. Instrumented observer-reference count is asserted greater than zero.
19. A failed observer proof fails the build rather than being suppressed.
20. Existing driver tests cover the split-library and hard-proof invariants.

### Provenance simplification

21. There is no divergent Python-vs-shell tracked-tree digest algorithm.
22. Preferably there is one Python implementation or the redundant tree digest is removed.
23. Exact pinned i2pd revision is asserted before build.
24. The tracked i2pd worktree is asserted clean before build.
25. Actual i2pr/instrumented/control binary digests are present in the compact result.
26. No new manifest hierarchy is added.

### Workflow/environment

27. The workflow remains one `ubuntu-24.04` job.
28. The trigger remains manual `workflow_dispatch` only.
29. There is no cross-job binary artifact upload/download.
30. i2pr and i2pd binaries are built and executed in the same workspace.
31. i2pd build parallelism remains bounded.
32. Live probes use literal `127.0.0.1` and network ID 99 only.
33. No public reseed/NetDB/SAM/I2CP/HTTP/SOCKS/SSU2 operation occurs.
34. No rootless namespace, Multipass, Docker, VM, or alternate lane is added.
35. No retry action exists.
36. Raw run state is deleted before evidence upload.
37. Only the compact sanitized development result is retained.

### Overengineering boundary

38. Active tracked Python LOC does not increase due to Plan 100 unless a direct replacement removes more plan-specific code in the same commit.
39. No deleted historical plan-specific Python runner/test is restored.
40. `scripts/check-ntcp2-interoperability.sh` remains a small durable-invariant checker rather than a plan-text validator.
41. No new candidate/certificate/reviewer/provenance architecture is introduced.
42. No new interoperability topology is introduced.
43. No Plan 101 NTCP2 cleanup roadmap is created.

### Final execution

44. A clean exit-ready commit is recorded before the live dispatch.
45. Exactly one initial Plan 099 manual run is dispatched from that commit.
46. If it fails pre-TCP, at most one narrow direct correction + one replacement run occurs.
47. If it fails post-TCP, exactly one unchanged reproduction occurs before any Rust correction.
48. At most one clearly owned Rust NTCP2 correction is made under this plan.
49. A remaining failure after the bounded correction is recorded and accepted as localized/blocked rather than spawning more apparatus.
50. NTCP2 remains disabled/non-advertised regardless of Plan 100 outcome.

### Planning/router handoff

51. `plans/099-status.md` no longer claims Plan 099 is fully passed before its final run result exists.
52. Plans 087/088 no longer direct active execution through Plan 095.
53. Plan 079 is recorded as deferred to the pre-activation/public-network checkpoint.
54. Historical evidence in Plans 087/088 is preserved rather than rewritten.
55. Active docs do not send the next agent back into the superseded Plan 095 artifact-transfer sequence.
56. Production daemon composition is explicitly authorized next for all three terminal outcomes.
57. Local/offline NetDB work is explicitly authorized next for all three terminal outcomes.
58. External NetDB over NTCP2 remains blocked unless the independent two-way smoke passes.
59. The next substantial roadmap is router composition + RouterInfo/NetDB foundation.
60. No further generalized NTCP2 evidence roadmap is registered.

## Small-model execution sequence

Execute exactly in this order.

### Phase A — baseline

1. Read Plan 100 completely.
2. Confirm HEAD and the current Plan 099 status.
3. Read only the active workflow, exit-gate module, i2pd build script, two focused test files, and status documents.
4. Do not inspect/revive deleted historical Python files from git history.

### Phase B — exit gate

5. Fix workflow/module import and keyword mismatches.
6. Reduce the result vocabulary to the three Plan 099 outcomes.
7. Make classification highest-stage-aware.
8. Add focused cases to the existing functional test owner.
9. Run only those tests.

### Phase C — reference build proof

10. Remove the undeclared `rtk` command dependency.
11. Add hard pristine/instrumented observer-reference assertions.
12. Remove suppressed proof semantics.
13. Remove the divergent source-digest fallback.
14. Run the existing i2pd-driver tests.

### Phase D — status pre-live

15. Mark Plan 099 as implementation-landed/exit-run-pending rather than passed.
16. Mark Plan 100 exit-ready only after the local validation passes.
17. Mark Plans 095/087/088 as historical/superseded active execution authority where necessary.
18. Keep Plan 079 deferred.

### Phase E — validation

19. Run the required workspace/focused baseline.
20. Reread the workflow API boundary manually.
21. Verify no new plan-specific Python file/test was added.
22. Verify the working tree is clean.
23. Commit the exit-ready candidate.

### Phase F — one run

24. Dispatch the Plan 099 workflow once.
25. Apply Branch A/B/C exactly.
26. Do not add a new plan for a failure.

### Phase G — close and leave Milestone 3 apparatus

27. Record the compact result and terminal status.
28. Reconcile the few active status docs once.
29. Stop NTCP2 planning.
30. Hand off immediately to the production router + NetDB roadmap.

## Expected final repository state

### Best case

```text
plan_099                     = closed-development-smoke-passed
plan_100                     = passed-exit-cleanup-and-handoff
development_interop          = passed
level2_repetition            = deferred-to-pre-activation-checkpoint
level3_release_qualification = pending/environment-constrained
ntcp2_normal_daemon          = disabled
ntcp2_advertised             = false
plan_095                     = historical
plan_087                     = historical-development-sequence
plan_088                     = historical-development-sequence
production_router            = next
netdb_foundation             = next
```

### Localized protocol defect

```text
plan_099                     = closed-protocol-defect-localized
plan_100                     = passed-exit-cleanup-and-handoff
development_interop          = protocol-defect-localized
exact_wire_stage             = recorded
ntcp2_normal_daemon          = disabled
ntcp2_advertised             = false
production_router            = next
local_netdb_foundation       = next
external_netdb_over_ntcp2    = blocked
```

### Environment/harness blocked after bounded correction

```text
plan_099                     = closed-environment-or-harness-blocked
plan_100                     = passed-exit-cleanup-and-handoff
development_interop          = environment-or-harness-blocked
no_new_lane                  = true
ntcp2_normal_daemon          = disabled
ntcp2_advertised             = false
production_router            = next
local_netdb_foundation       = next
external_netdb_over_ntcp2    = blocked
```

## Closure rule

Plan 100 is successful when the repository stops treating interoperability infrastructure as the product.

The final development smoke is useful because it answers one bounded compatibility question against a real independent implementation. It is not valuable enough to justify another sequence of plan-specific Python validators, CI artifact machinery, or environment recovery systems.

After the bounded Plan 100 exit, the project returns to its primary objective: implementing a Rust I2P router.