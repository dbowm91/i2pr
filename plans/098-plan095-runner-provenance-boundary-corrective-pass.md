# Plan 098: Plan 095 runner/provenance boundary corrective pass

## Status and authority

- Status: planned; single narrow corrective pass.
- Parent roadmap: Plan 085.
- Active closure plan: Plan 095.
- Immediate predecessor corrective plans: Plan 096 and Plan 097.
- Planning baseline: `e9badcf166dec180cbdcd29c1a7a57faade122c4` or a clean descendant containing the same authoritative Plan 095 live-run record.
- Authoritative reference revision: i2pd 2.60.0, `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Development topology: `host-loopback-development` only.
- Closure target: repair the runner/provenance ownership boundary exposed by the first authoritative Plan 095 live attempt, prove the corrected boundary locally/staticly, and leave the repository ready for exactly one subsequent manual Plan 095 dispatch.
- Next executable action after Plan 098 closes: one manual Plan 095 CI dispatch from the exact clean Plan 098 implementation commit.
- Plan 088 remains blocked until Plan 095 produces one passing instrumented forward record and one passing control forward record from the same authoritative CI evidence pair.
- NTCP2 remains experimental and non-advertised.

Active sequence:

```text
Plan 085 -> Plan 086 -> Plan 087 -> Plan 090 -> Plan 091
         -> Plan 092 -> Plan 093 -> Plan 094
         -> Plan 095 CI lane
         -> Plan 096 workflow correction
         -> Plan 097 artifact/cleanup correction
         -> first authoritative Plan 095 live attempt
              result: pre_protocol_rejected /
                      pre-protocol-preparation-failed
         -> Plan 098 runner/provenance correction
         -> exactly one manual Plan 095 rerun
         -> Plan 088 only after Plan 095 forward closure
```

Plan 098 is intentionally one coherent ownership-boundary pass. Do not split the listed defects into separate CI attempts. They are adjacent manifestations of the same contract problem: the wrapper measures one set of executable/provenance inputs while the runner reconstructs or substitutes different inputs internally.

## Executive finding

The first authoritative Plan 095 run on 2026-08-10 successfully progressed through the CI contract and deterministic build jobs and launched the `forward-instrumented` live runner. Its terminal record was:

```text
terminal_result = pre_protocol_rejected
reason_code     = pre-protocol-preparation-failed
```

That outcome must **not** be treated as evidence of an NTCP2 handshake, transport, cryptographic, or data-phase failure.

The immediate deterministic cause is visible in the current repository:

1. The Plan 095 workflow builds/downloads the authoritative i2pr binary at:

   ```text
   $GITHUB_WORKSPACE/target/interop/plan095-build/output/i2pr-interop
   ```

2. The workflow supplies that exact path to the wrapper with `--i2pr-binary`.
3. The wrapper validates that the supplied file exists and hashes those exact bytes.
4. `_run_forward_probe()` passes only the resulting digest into `plan083_runner.execute_real_probe()`.
5. `execute_real_probe()` ignores the supplied executable path because no executable-path parameter exists and instead reconstructs:

   ```python
   repo_root / "target" / "debug" / "i2pr-interop"
   ```

6. A fresh live GitHub Actions job does not build `target/debug/i2pr-interop`; the authoritative artifact exists only under the Plan 095 build-output tree.
7. The runner therefore returns `pre-protocol-preparation-failed` before executing `i2pr-interop ntcp2 prepare`, before starting i2pd, before TCP, and before any NTCP2 wire activity.

This same ownership defect exists in the reverse runner and preflight helper, and there are adjacent provenance-role defects that would make a future passing evidence pair invalid if only the immediate path mismatch were fixed.

Plan 098 corrects all of those adjacent issues in one pass before consuming another authoritative CI attempt.

## Current defects owned by Plan 098

### Defect A — authoritative i2pr executable path is lost at the wrapper/runner boundary

Current behavior:

```text
workflow authoritative binary path
    -> wrapper validates path
    -> wrapper hashes path
    -> runner receives hash only
    -> runner reconstructs target/debug/i2pr-interop
```

Required behavior:

```text
workflow authoritative binary path
    -> wrapper validates exact path
    -> wrapper hashes exact path
    -> runner receives exact path + exact hash
    -> runner verifies path/hash binding
    -> same exact path executes prepare
    -> same exact path executes validate-scenario
    -> same exact path executes dial/listen
```

No runner may infer a build location from `repo_root` for an attempted live record after Plan 098.

### Defect B — reverse and preflight surfaces repeat the same path reconstruction

`plan084_runner.py` and `preflight_runner.py` currently search `target/debug` or `target/release` instead of consuming the explicit authoritative artifact supplied by the caller.

Even though Plan 088 remains blocked and Plan 098 must not run the reverse direction, the shared ownership contract must be corrected now so Plan 088 does not immediately reproduce the same pre-protocol blocker.

### Defect C — i2pr build-manifest provenance is populated from the i2pd manifest digest

`execute_real_probe()` currently carries one `build_manifest_sha256` argument and assigns it to both:

```text
i2pr_build_manifest_sha256
i2pd_build_manifest_sha256
```

This is provenance-invalid. The i2pr artifact and the i2pd driver artifact have different build manifests and must carry separately measured digests.

### Defect D — control execution selects the instrumented i2pd build manifest

The wrapper currently derives `build-manifest-instrumented.json` whenever an i2pd driver binary is supplied. A future control attempt therefore risks executing the control driver while recording the instrumented driver's manifest digest.

The role-to-manifest mapping must be exact and fail closed:

```text
i2pd_ntcp2_interop_driver_instrumented
    -> build-manifest-instrumented.json

i2pd_ntcp2_interop_driver_control
    -> build-manifest-control.json

anything else
    -> reject before runner execution
```

### Defect E — underlying runner record hard-codes `attempt_kind=instrumented`

The control workflow may sanitize an output under a control schema, but the underlying Plan 083 probe record currently hard-codes the attempt role as `instrumented`.

The authoritative underlying record must carry the real role. Sanitization may not relabel a semantically incorrect source record.

Allowed values for the forward Plan 095 path are exactly:

```text
instrumented
control
```

### Defect F — source-tree digest algorithm is inconsistent across the workflow and build-driver manifest

The workflow was corrected to compute the pinned i2pd source-tree digest from tracked files, excluding `.git` administrative state. `build-driver.sh` still computes its `PRISTINE_TREE_SHA` using recursive filesystem enumeration, which includes checkout metadata.

The build manifests are the source from which the wrapper later obtains `reference_source_tree_sha256`; therefore the live record can currently carry a different/non-canonical identity from the workflow's canonical tracked-tree digest.

One canonical algorithm must own source-tree identity across:

```text
workflow recorded source-tree digest
build-manifest-instrumented.json
build-manifest-control.json
wrapper provenance
instrumented probe record
control probe record
final Plan 095 gate
```

### Defect G — final gate checks nonzeroness but not full exact provenance equivalence

The final Plan 095 gate must prove that the record claims correspond to the actual downloaded artifacts and role-specific manifests. A record with nonzero but incorrect provenance must fail closed.

### Defect H — status authority misclassifies/stales the authoritative attempt

`plans/088-status.md` records the first authoritative attempt, but its wording describes the pre-protocol rejection as a protocol-level classification. `plans/087-status.md` still carries the older `awaiting-one-authoritative-run` token.

Plan 098 must reconcile these status surfaces without claiming Plan 095 closure.

## Scope lock

Plan 098 may edit only the smallest surfaces necessary for the ownership/provenance correction, expected primarily:

```text
scripts/interop/run-minimal-i2pd-host-loopback-probe.py
tests/integration/ntcp2/harness/plan083_runner.py
tests/integration/ntcp2/harness/plan084_runner.py
tests/integration/ntcp2/harness/preflight_runner.py
tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh
.github/workflows/ntcp2-interop-host-loopback-development.yml
scripts/check-plan095-workflow.sh
scripts/check-ntcp2-interoperability.sh
focused harness tests
plans/087-status.md
plans/088-status.md
README.md / AGENTS.md / interop skill only if required for status authority
```

Do **not** modify the NTCP2 cryptographic implementation, Noise state machine, frame codec, data-phase receive oracle, RouterInfo parser/signature behavior, transport activation policy, support advertisement, normal daemon behavior, or public-network surfaces unless a new post-correction live record directly demonstrates such a defect. Plan 098 itself must not produce that record.

## Non-goals

Plan 098 must not:

- run another Plan 095 live attempt as part of implementation;
- run Plan 088;
- change NTCP2 handshake or data-phase logic;
- weaken exact Router Hash, RouterInfo, DeliveryStatus, observer-generation, or frame predicates;
- introduce retries;
- increase timeouts to hide a deterministic failure;
- introduce Docker, namespaces, Multipass, public I2P, reseed, DNS, SAM, I2CP, HTTP, SOCKS, or SSU2;
- introduce release qualification from host-loopback evidence;
- advertise NTCP2 support;
- replace exact binary/path provenance with an environment-dependent search path;
- accept a fallback binary if the explicit authoritative binary is missing;
- allow the control run to reuse the instrumented role or manifest;
- write a passing Plan 093/087/095 status before a real passing evidence pair exists.

## Mandatory post-Plan-098 invariants

```text
i2pr executable path source         = explicit caller-supplied path only
i2pr executable path fallback       = forbidden for live attempts
i2pr executable SHA                 = measured from exact executed file
i2pr build manifest                 = explicit, distinct, nonzero digest
i2pd driver path                    = explicit caller-supplied path
i2pd driver SHA                     = measured from exact executed file
i2pd build manifest                 = exact role-specific manifest
i2pd source-tree digest             = one canonical tracked-file identity
attempt_kind                        = exact instrumented | control
attempt_index                       = 1
instrumented/control role aliasing  = forbidden
manifest role aliasing              = forbidden
binary/manifest digest mismatch     = fail closed before pass
source-tree digest mismatch         = fail closed before pass
source commit                       = exact checked-out 40-hex commit
reference revision                  = exact pinned i2pd revision
topology                            = host-loopback-development
network ID                          = 99
release_qualified                   = false
isolation_qualified                 = false
NTCP2 support                       = experimental-non-advertised
Plan 088                            = blocked until Plan 095 closure
```

# Execution work packages

## WP0 — freeze and classify the baseline

Before code changes:

1. Record current `main` HEAD.
2. Confirm the authoritative 2026-08-10 Plan 095 attempt is represented in repository status authority.
3. Confirm the observed result is exactly:

   ```text
   terminal_result = pre_protocol_rejected
   reason_code = pre-protocol-preparation-failed
   ```

4. Confirm no later passing Plan 095 instrumented/control pair already exists.
5. Confirm the current forward runner still reconstructs `repo_root/target/debug/i2pr-interop`.
6. Confirm the reverse runner/preflight still reconstruct their own i2pr binary location.
7. Confirm the wrapper receives `--i2pr-binary` but does not thread its path to the runner.
8. Confirm one i2pd build-manifest digest is still reused as the i2pr build-manifest digest.
9. Confirm the underlying forward runner still hard-codes `attempt_kind=instrumented`.
10. Confirm `build-driver.sh` source-tree hashing still differs from the canonical workflow algorithm.

Write the implementation commit message/status notes from the measured baseline rather than copying stale plan text.

Acceptance criteria:

- all eight defects are either confirmed or explicitly marked already corrected by a later commit;
- no already-corrected behavior is rewritten unnecessarily;
- the August 10 attempt is classified as **pre-protocol runner/provenance rejection**, not wire-level NTCP2 failure.

## WP1 — make the explicit i2pr binary path first-class runner input

Extend the real runner APIs to accept the authoritative i2pr executable path explicitly.

Preferred API shape:

```python
execute_real_probe(
    ...,
    i2pr_binary: Path,
    i2pr_binary_sha256: str,
    i2pr_build_manifest_sha256: str,
    ...,
)
```

Mirror the same ownership for:

```python
execute_reverse_probe(..., i2pr_binary: Path, ...)
preflight execution helpers(..., i2pr_binary: Path, ...)
```

The exact names may follow existing project style, but the semantic contract must be explicit.

At runner entry, before any state preparation:

1. require an absolute path for attempted live execution;
2. require the path to be a regular file;
3. require it to be executable where the placement needs executability;
4. reject symlink substitution if the current artifact contract rejects symlinks;
5. hash the file bytes;
6. compare the measured hash to the supplied `i2pr_binary_sha256`;
7. fail closed with a typed pre-protocol provenance/preparation reason if mismatched;
8. retain the exact path object for all later i2pr subprocess starts.

For host-loopback development, the same exact binary path must own:

```text
ntcp2 prepare
ntcp2 validate-scenario
ntcp2 dial / responder launch
```

Forbidden after WP1 in live runners:

```python
repo_root / "target" / "debug" / "i2pr-interop"
repo_root / "target" / "release" / "i2pr-interop"
shutil.which("i2pr-interop")
PATH search
first-existing fallback lists
```

Those forms may remain only in isolated legacy/unit helper code that cannot produce an attempted-live authoritative record; if retained, prove the separation in tests.

Acceptance criteria:

- a live runner can execute when the only i2pr binary exists at a caller-supplied arbitrary absolute artifact path;
- absence of `target/debug/i2pr-interop` does not cause rejection when the explicit artifact exists;
- changing the explicit file after digest measurement is detected and rejected;
- the forward, reverse, and preflight ownership contracts are aligned;
- no live path silently falls back to a repo target directory.

## WP2 — thread the explicit binary path through the wrapper

The wrapper already accepts `--i2pr-binary`. Make that argument authoritative rather than digest-only metadata.

Required behavior:

```text
CLI --i2pr-binary
  -> input validation
  -> canonical absolute path
  -> exact SHA measurement
  -> exact runner parameter
  -> exact process execution
```

The wrapper must not re-resolve the binary through `repo_root` after validation.

For non-preflight attempted-live calls:

- `--i2pr-binary` remains mandatory;
- the path must exist before the runner is called;
- provenance must be measured from that exact path;
- the exact same path must be passed into the runner.

Preflight should also use the explicit binary path when supplied by the Plan 095 apparatus. If any legacy operator preflight mode permits omission, it must be clearly non-authoritative and structurally incapable of producing a passing Plan 095 evidence record; prefer requiring the explicit binary for all Plan 095-related preflight use.

Acceptance criteria:

- wrapper tests assert object/path identity is threaded to the runner;
- the digest and executable path refer to the same file;
- no wrapper path computes a digest for file A while runner executes file B.

## WP3 — separate i2pr and i2pd build-manifest provenance

Introduce distinct inputs:

```text
i2pr_build_manifest_sha256
i2pd_build_manifest_sha256
```

Do not use one generic `build_manifest_sha256` for both artifact classes in attempted-live records.

Wrapper responsibilities:

1. Locate the i2pr build manifest beside the supplied Plan 095 i2pr artifact using the exact bounded artifact layout:

   ```text
   i2pr-build-manifest.json
   ```

2. Validate the manifest is a regular file.
3. Parse it.
4. Confirm its recorded `i2pr_binary_sha256` equals the measured supplied i2pr binary SHA.
5. Confirm its `source_commit` equals the requested source commit.
6. Hash the manifest bytes and pass that digest as `i2pr_build_manifest_sha256`.

Runner responsibilities:

- write the distinct i2pr manifest digest to `i2pr_build_manifest_sha256`;
- write the role-specific i2pd manifest digest to `i2pd_build_manifest_sha256`;
- reject zero placeholders for an attempted-live record;
- never substitute one for the other.

Acceptance criteria:

- changing only the i2pr manifest changes only the i2pr manifest provenance field;
- changing only the i2pd manifest changes only the i2pd field;
- setting both fields equal via accidental aliasing is caught by fixture tests unless the fixture bytes genuinely happen to hash identically;
- a passing record cannot be built with the i2pd manifest digest in the i2pr field.

## WP4 — bind i2pd driver role to exact manifest and attempt kind

Determine the role from an explicit caller argument, not by sanitization after the fact.

Recommended wrapper CLI/API addition:

```text
--attempt-kind instrumented|control
```

If the workflow wants role inference from the exact binary filename, use that only as a cross-check, not the sole authority.

Required mapping:

```text
attempt_kind=instrumented
  driver = i2pd_ntcp2_interop_driver_instrumented
  manifest = build-manifest-instrumented.json

attempt_kind=control
  driver = i2pd_ntcp2_interop_driver_control
  manifest = build-manifest-control.json
```

Require all three dimensions to agree:

```text
requested attempt_kind
actual driver identity/path
selected manifest identity/content
```

A disagreement is a typed pre-protocol provenance rejection and must not be normalized later by the sanitized evidence layer.

Modify `execute_real_probe()` so `attempt_kind` is an explicit validated input and the underlying probe record uses that exact value.

The control job must therefore produce an underlying source record containing:

```text
attempt_kind = control
attempt_index = 1
```

The instrumented job must produce:

```text
attempt_kind = instrumented
attempt_index = 1
```

Acceptance criteria:

- instrumented binary + control role => rejected;
- control binary + instrumented role => rejected;
- control binary + instrumented manifest => rejected;
- instrumented binary + control manifest => rejected;
- role-specific happy-path fixture passes;
- sanitized output preserves rather than invents the attempt role.

## WP5 — canonicalize pinned i2pd source-tree identity end-to-end

Replace `build-driver.sh`'s recursive filesystem digest with the same canonical tracked-source identity used by the workflow.

The implementation must be deterministic and exclude `.git` administrative metadata.

Preferred contract:

```text
for each tracked path in stable git order:
    append path + NUL
    append SHA256(file bytes) + NUL
SHA256(the resulting canonical byte stream)
```

If the project already has a helper implementing the Plan 096 algorithm, reuse it rather than maintaining two shell copies.

Mandatory assertions in the build job:

1. exact pinned revision is checked out;
2. canonical tracked-tree digest is computed once as source authority;
3. `build-manifest-instrumented.json.reference_source_tree_sha256` equals that digest;
4. `build-manifest-control.json.reference_source_tree_sha256` equals that digest;
5. both manifests carry the exact pinned revision;
6. wrapper-provided `reference_tree_sha256` equals that same digest.

Acceptance criteria:

- modifying `.git` administrative data does not change source-tree digest;
- modifying tracked source bytes changes the digest;
- adding an untracked build file does not change the digest;
- changing a tracked path/content changes the digest;
- instrumented/control manifests agree exactly on source-tree digest and revision;
- the workflow and build script no longer use competing algorithms.

## WP6 — strengthen Plan 095 final gate to exact provenance equivalence

The final gate must validate source record claims against actual downloaded build artifacts, not only test for nonzero strings.

For both instrumented and control records, validate at minimum:

```text
source_commit == GITHUB_SHA
reference_revision == pinned revision
topology_kind == host-loopback-development
network_id == 99
attempt_index == 1
cleanup_result == clean
terminal_result == passed

i2pr_binary_sha256 == SHA256(actual downloaded i2pr binary)
i2pr_build_manifest_sha256 == SHA256(actual i2pr manifest)
reference_source_tree_sha256 == canonical pinned-tree digest
scenario_sha256 is nonzero 64-hex
placement_record_sha256 is nonzero 64-hex
```

Instrumented-only:

```text
attempt_kind == instrumented
i2pd_binary_sha256 == SHA256(instrumented driver)
i2pd_build_manifest_sha256 == SHA256(build-manifest-instrumented.json)
```

Control-only:

```text
attempt_kind == control
i2pd_binary_sha256 == SHA256(control driver)
i2pd_build_manifest_sha256 == SHA256(build-manifest-control.json)
```

Cross-record equivalence:

```text
same source_commit
same reference_revision
same canonical reference source-tree digest
same i2pr binary digest
same i2pr build-manifest digest
same topology/network contract
same protocol acceptance predicates
role-specific i2pd binary/manifest differences exactly as expected
```

Do not require the instrumented and control i2pd binary hashes to match; they are intentionally different artifacts. Require each to match its own expected build output.

Acceptance criteria:

- a synthetically altered record digest fails the gate;
- swapping instrumented/control manifests fails;
- swapping role labels fails;
- swapping i2pr binary or manifest fails;
- zero placeholders fail;
- correct role-specific records pass the static/unit gate fixture.

## WP7 — add functional runner-boundary regressions

Do not rely only on grep/string assertions. Add focused temporary-filesystem tests that execute the Python runner boundary with realistic paths/fakes.

Minimum test cases:

### Test A: explicit artifact path succeeds without target/debug

Create a temporary repo layout such that:

```text
<repo>/target/debug/i2pr-interop        = absent
<artifact-root>/i2pr-interop            = present
```

Supply the explicit artifact path and matching hash. Stub only the external subprocess behavior necessary to reach/verify preparation ownership. Assert the runner uses the explicit path and does not reject solely because `target/debug` is absent.

### Test B: digest/path mismatch fails closed

Measure file A, then change/replace its bytes before runner entry. Assert typed pre-protocol provenance/preparation rejection before external process launch.

### Test C: wrapper threads exact path

Patch/mock the runner entry and assert the exact resolved `Path` object/value passed from `--i2pr-binary` reaches it.

### Test D: i2pr/i2pd manifests remain distinct

Use two synthetic manifests with different digests and assert both appear in the correct record fields.

### Test E: control role is real in underlying record

Run the source-record builder using `attempt_kind=control`; assert the underlying record—not merely sanitized evidence—contains `control`.

### Test F: role/manifest swaps fail

Exercise all mismatched combinations.

### Test G: canonical source-tree digest ignores `.git`

Use a temporary Git repository; mutate `.git` metadata and untracked files without changing tracked files; assert digest stability. Change a tracked file; assert digest changes.

### Test H: reverse path ownership

Assert `execute_reverse_probe` accepts/uses the supplied artifact path even though Plan 088 itself is not executed.

### Test I: preflight path ownership

Assert Plan 095-related preflight uses the supplied artifact path and does not silently reconstruct a repo build path.

Acceptance criteria:

- all new tests fail against the pre-Plan-098 implementation for the intended reason;
- all pass after correction;
- no test requires public networking or privileged features;
- no test weakens protocol predicates.

## WP8 — update static boundary checks

Extend the existing Plan 095/NTCP2 static checks to reject regression of the ownership contract.

At minimum reject:

```text
live plan083/084 reconstruction of target/debug/i2pr-interop
attempted-live fallback to target/release or PATH search
one generic manifest digest assigned to both i2pr/i2pd fields
hard-coded attempt_kind=instrumented in a role-parametric forward runner
control selection of build-manifest-instrumented.json
find "$I2PD_SOURCE_DIR" -type f as source-tree authority
workflow final gate lacking exact role checks
```

Prefer semantic/unit tests for nuanced behavior and keep grep/static checks limited to architectural invariants that are cheap and stable.

Acceptance criteria:

- static checks catch reintroduction of the exact high-risk anti-patterns;
- checks do not become a second parser for the full implementation;
- Plan 098 does not materially increase unrelated CI complexity.

## WP9 — reconcile status authority

Update `plans/087-status.md` and `plans/088-status.md` consistently.

Required classification after Plan 098 implementation and before the next live run:

```text
plan_095 = active-runner-provenance-corrected-awaiting-authoritative-rerun
plan_098 = passed-runner-provenance-boundary-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

Exact token spelling may follow existing repository status-token conventions, but both status files must agree.

Correct the August 10 wording to state:

```text
The run was authoritative as a CI execution attempt.
It reached the live runner.
It failed before i2pr state preparation executed because the runner
reconstructed a non-authoritative target/debug binary path.
No TCP or NTCP2 wire-level conclusion is supported by that result.
```

Preserve the historical record rather than deleting it.

Do not create `plans/093-status.md` as passed and do not close Plan 087.

Acceptance criteria:

- Plan 087 and Plan 088 status tokens agree;
- no status says Plan 095 is still awaiting its *first* authoritative run;
- no status calls the August 10 pre-protocol path failure an NTCP2 protocol failure;
- no downstream plan is unblocked.

## WP10 — validation before commit

Run the narrow focused tests first, then the established repository gates.

Minimum expected commands, adapted to actual test filenames after implementation:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan098.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan095.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan097.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
bash scripts/check-plan095-workflow.sh
bash scripts/check-ntcp2-interoperability.sh
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.95.0 doc --locked --workspace --no-deps
git diff --check
```

If an existing repository gate is demonstrably unrelated and environment-blocked, record the exact blocker; do not silently skip it or weaken the gate.

Acceptance criteria:

- focused Plan 098 tests pass;
- prior Plan 093-097 regression surfaces remain green;
- workflow audit passes;
- static interop boundary passes;
- Rust workspace gates remain green;
- tree is clean after commit.

## WP11 — commit the correction before any live execution

The implementation and its tests/status reconciliation must be committed before the next authoritative Plan 095 run.

Suggested commit message:

```text
interop: correct Plan 095 runner binary and provenance ownership
```

The implementation commit should summarize:

```text
explicit i2pr binary path ownership
exact path/hash verification
separate i2pr/i2pd build manifests
instrumented/control manifest binding
explicit attempt_kind
canonical i2pd tracked-tree digest
exact final-gate provenance checks
functional regression coverage
status reclassification of August 10 attempt
```

Record the exact commit SHA as the source commit for the subsequent manual workflow dispatch.

Acceptance criteria:

- no uncommitted source/test/status changes remain;
- the next Plan 095 run can be bound to one exact clean correction commit;
- no live run occurred before the commit.

# Post-Plan-098 execution boundary

Plan 098 ends **before** another live CI run.

After Plan 098 is implemented and committed, perform exactly one manual dispatch of:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
```

against the exact Plan 098 correction commit.

The expected execution sequence remains:

```text
contract
  -> build
  -> forward-instrumented
       -> if and only if exact pass
          forward-control
            -> validate-gate
```

No automatic retry.

## Classification of the next authoritative run

### Case 1 — pre-protocol failure remains

If the next run fails before listener/TCP:

1. retain sanitized evidence;
2. identify the exact typed pre-protocol owner;
3. stop;
4. do not classify it as NTCP2 wire failure;
5. do not run control if instrumented did not pass;
6. create a new corrective plan only for a newly demonstrated defect.

### Case 2 — instrumented reaches listener/TCP/NTCP2 and fails

This is the first point where a new wire-stage diagnosis may be justified.

Record the highest exact stage and reason. Do not revive historical Plan 092 classifications automatically.

### Case 3 — instrumented passes, control fails

Classify as instrumented/control divergence. Preserve both records. Do not close Plan 087. Investigate whether observer instrumentation affects external behavior or whether the control provenance/runtime differs.

### Case 4 — instrumented and control both pass, final gate passes

Only then reconcile:

```text
Plan 093 = passed
Plan 094 = passed-via-plan095-ci-closure
Plan 095 = passed
Plan 096 = passed
Plan 097 = passed
Plan 098 = passed
Plan 087 = passed
Plan 088 = next-executable / not-yet-run
Plan 079 = blocked-pending-plan088-two-way-pass
Plan 072 = inactive-pending-plan088-ambiguity
NTCP2 = experimental-non-advertised
```

Then move to Plan 088 reverse development interoperability using the corrected explicit binary/provenance ownership contract.

# Explicit closure criteria for Plan 098

Plan 098 is complete only when every item below is true:

1. The forward attempted-live runner receives an explicit i2pr executable path.
2. The forward runner no longer reconstructs `target/debug/i2pr-interop` for authoritative attempted-live execution.
3. The reverse attempted-live runner receives the same explicit-path contract.
4. Plan 095-related preflight receives/uses the explicit-path contract.
5. The exact executed i2pr bytes are re-hashed and matched to `i2pr_binary_sha256` before process execution.
6. The i2pr build manifest is separately measured and validated against the i2pr binary/source commit.
7. The i2pd build manifest is separately measured and bound to the exact role-specific driver.
8. Instrumented and control binary/manifest mappings are exact and fail closed on mismatch.
9. `attempt_kind` is explicit and the underlying control source record says `control`.
10. `attempt_index` remains exactly `1`.
11. The pinned i2pd source-tree digest uses one canonical tracked-file algorithm across build script, workflow, manifests, wrapper, records, and gate.
12. `.git` administrative metadata does not affect the canonical source-tree digest.
13. The final gate validates record digests against actual downloaded binaries and manifests.
14. The final gate validates role-specific instrumented/control identities.
15. The final gate validates same i2pr/source/reference identity across the evidence pair.
16. Functional tests prove an arbitrary explicit artifact path works with `target/debug` absent.
17. Functional tests prove path/hash mutation fails closed.
18. Functional tests prove manifest and role swaps fail closed.
19. Existing Plan 093-097 regression tests remain green.
20. Static workflow/interoperability checks remain green.
21. Rust workspace verification remains green.
22. `plans/087-status.md` and `plans/088-status.md` agree on current authority.
23. The August 10 result is documented as a pre-protocol runner/provenance failure with no TCP/NTCP2 wire conclusion.
24. Plan 087 remains open.
25. Plan 088 remains blocked.
26. NTCP2 remains experimental and non-advertised.
27. All implementation/test/status changes are committed on a clean tree.
28. **No new Plan 095 live dispatch has been performed inside Plan 098.**

# Smaller-model execution guidance

Execute the plan in this order and do not skip ahead:

```text
A. Read baseline runner/wrapper/workflow/build manifests/status files.
B. Write failing focused tests for explicit binary path ownership.
C. Implement explicit binary path threading forward/reverse/preflight.
D. Write failing tests for distinct build manifests.
E. Implement distinct i2pr/i2pd manifest provenance.
F. Write failing role-swap/control-attempt tests.
G. Implement explicit attempt_kind + exact role/manifest binding.
H. Write failing canonical source-tree digest tests.
I. Replace build-driver source digest with canonical tracked-tree identity.
J. Strengthen final gate exact provenance checks.
K. Add/update static regression checks.
L. Reconcile Plan 087/088 status wording/tokens.
M. Run focused tests.
N. Run full existing interop regression surface.
O. Run Rust/static gates.
P. Commit all corrections.
Q. STOP. Do not dispatch Plan 095 in this implementation task.
```

When a test exposes an unrelated pre-existing defect, record it separately. Do not broaden Plan 098 into protocol redesign or general CI cleanup.

# Handoff state expected after implementation

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = active-runner-provenance-corrected-awaiting-authoritative-rerun
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_098 = passed-runner-provenance-boundary-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised

next_action = exactly-one-manual-plan095-dispatch-from-plan098-correction-commit
```

This is the only intended handoff. Plan 098 does not authorize reverse execution, release claims, support advertisement, retries, or protocol changes.