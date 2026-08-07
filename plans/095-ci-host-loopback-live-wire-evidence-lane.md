# Plan 095: CI host-loopback live-wire evidence lane

## Status and authority

- Status: planned; corrective execution-lane successor.
- Parent roadmap: Plan 085.
- Immediate predecessor: Plan 094.
- Closure target: complete the live-wire portions of Plan 094 and Plan 093 so Plan 087 can close.
- Next roadmap plan after successful closure: Plan 088.
- Primary execution environment: GitHub Actions standard `ubuntu-24.04` hosted runner.
- Development topology: existing `host-loopback-development` only.
- Reference implementation: source-locked i2pd 2.60.0 revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Baseline implementation commit: `0e40f403b822907c84acb5ccb669d41d8879ca05` or a clean descendant containing the Plan 094 implementation surface.
- Blocks: Plan 088, Plan 079, and every claim of two-way NTCP2 development interoperability until the required CI evidence pair passes and status authority is reconciled.
- Plan type: CI execution-lane correction, deterministic build/provenance binding, one instrumented forward attempt, one control forward attempt, artifact sanitation/validation, and Plan 087 -> Plan 088 handoff.

Active sequence:

```text
Plan 085 -> Plan 086 -> Plan 087 -> Plan 090 -> Plan 091
         -> Plan 092 -> Plan 093 -> Plan 094 (implementation landed;
                                        live closure environment-blocked)
         -> Plan 095 (CI host-loopback live-wire closure)
         -> Plan 088
         -> conditional Plan 079 or Plan 072
```

Plan 095 supersedes the Plan 094 assumption that a Plan 046 rootless sealed namespace or Plan 048/049 Multipass guest must become runnable before development-only forward evidence can close. Those lanes remain valid historical/qualification lanes, but they are **not prerequisites** for the `host-loopback-development` interoperability evidence needed to advance the current roadmap.

Plan 095 does not relax release or isolation claims. Evidence produced here remains development-only:

```text
development_only   = true
release_qualified  = false
isolation_qualified = false
```

## Executive finding

The current interactive execution environment cannot run the required live-wire closure path. This is an execution-environment limitation, not a newly demonstrated NTCP2 protocol failure.

The Plan 094 implementation commit `0e40f403b822907c84acb5ccb669d41d8879ca05` already landed the important pre-live corrections:

```text
real process invocation IDs
canonical event authority corrections
exact-target pass predicates
build/source/scenario provenance fields
zero-digest pass rejection
Plan 094 focused regression matrix
current Plan 094 gate authority
```

The remaining blocker is execution of the exact forward evidence pair.

The `host-loopback-development` topology itself does not require privileged networking. It needs only two ordinary userspace processes communicating over literal IPv4 loopback on fresh ports. Therefore the live portion should move to a reproducible GitHub Actions `ubuntu-24.04` VM rather than continuing to depend on unavailable local sandbox features.

GitHub-hosted runners execute each job on a fresh VM selected by `runs-on`; `ubuntu-24.04` is an available standard Linux label. Workflow artifacts may be uploaded by one job and downloaded by dependent jobs, and `needs` provides explicit job sequencing. Plan 095 should use those capabilities only for ordinary build/test/artifact transfer and must not rely on privileged containers, host networking, namespaces, or host mutation.

## Research basis

Implementation must preserve these GitHub Actions constraints:

1. Use `runs-on: ubuntu-24.04` for the primary lane.
2. Use `workflow_dispatch` as the initial trigger so live development interoperability remains explicit and bounded.
3. Use `permissions: contents: read` unless a later evidence-publication mechanism has a documented need for more.
4. Use `needs` to serialize jobs where one result gates the next.
5. Use workflow artifacts to transfer binaries, build manifests, and sanitized evidence between jobs when required.
6. Construct filesystem paths from the checked-out workspace or workflow environment; do not assume a persistent host path across jobs.
7. Treat every job as a fresh machine. Any state needed by a later job must be rebuilt deterministically or passed as a validated artifact.
8. Do not depend on GitHub-hosted runner implementation details beyond documented standard runner behavior.

## Environment contract

The Plan 095 CI lane must satisfy:

```text
runner                           = github-hosted ubuntu-24.04
architecture                     = x86_64 unless workflow explicitly records otherwise
topology_kind                    = host-loopback-development
development_only                 = true
release_qualified                = false
isolation_qualified              = false
bind address                     = literal 127.0.0.1 only
network_id                       = 99
fresh port                       = required per attempt
fresh run root                   = required per attempt
fresh identities                 = required per attempt
fresh invocation IDs             = required per process launch
fresh observer generation        = required per instrumented listener
public I2P network               = forbidden
reseed/bootstrap                 = forbidden
DNS                              = forbidden
SAM/I2CP/HTTP/SOCKS              = forbidden
SSU2                             = forbidden
sudo                             = forbidden
setcap                           = forbidden
ip netns                         = forbidden
nftables/iptables mutation       = forbidden
unshare/userns dependency        = forbidden
Multipass                        = forbidden
Docker requirement               = forbidden
--network host                   = forbidden
privileged containers            = forbidden
normal daemon NTCP2 enablement   = forbidden
support advertisement            = forbidden
```

The workflow may install ordinary build dependencies only when required by the existing reference-build contract. Installing build packages is not permission to introduce privileged network manipulation.

## Objective

Deliver a reproducible CI lane that closes Plan 094 in this exact order:

1. preserve all Plan 094 runner/provenance corrections;
2. add a dedicated manual CI workflow for development-only host-loopback interop;
3. run the full static/unit contract before any live attempt;
4. build and freeze i2pr plus instrumented/control i2pd artifacts from exact source revisions;
5. validate binary/manifests/provenance before live execution;
6. run exactly one instrumented forward attempt;
7. fail closed and stop if instrumented does not pass;
8. run exactly one control forward attempt only after instrumented passes;
9. validate instrumented/control external equivalence;
10. upload only sanitized evidence and deterministic build manifests;
11. retain a compact CI gate record binding workflow run metadata to the evidence pair;
12. after a successful CI run, create Plan 093 closure status, close Plan 087, and mark Plan 088 `next_executable`;
13. reuse the same CI host-loopback lane architecture for Plan 088 reverse execution in a later plan, without executing Plan 088 as part of Plan 095.

## Non-goals

Plan 095 must not:

- reopen Plan 093's NTCP2 data-phase implementation without direct new protocol evidence;
- restore Plan 092's superseded Branch A diagnosis;
- make rootless user namespaces or Multipass work;
- use a self-hosted runner as the first-choice path;
- introduce Docker solely to obtain process isolation;
- use public I2P, reseeding, or external peers;
- mutate host routing/firewall/network namespaces;
- weaken exact RouterInfo, Router Hash, network ID, DeliveryStatus, authenticated-frame, observer-generation, or provenance predicates;
- add automatic retries to obtain a passing run;
- run multiple instrumented attempts in one workflow execution cycle;
- run control after a failed instrumented attempt;
- run Plan 088 in the same workflow run;
- treat GitHub-hosted CI as release-qualified isolation evidence;
- advertise NTCP2 support.

## Mandatory invariants

```text
reference revision                        = exact pinned i2pd revision
workflow source commit                    = exact checked-out 40-hex commit
workflow dispatch ref                     = recorded
runner image label                        = recorded
runner architecture                       = recorded
all attempted-live binary digests         = exact nonzero 64-hex
all attempted-live build-manifest digests = exact nonzero 64-hex
reference source-tree digest               = exact nonzero 64-hex
scenario digest                            = exact nonzero 64-hex
placement digest                           = exact nonzero 64-hex
RouterInfo signature validation            = unchanged and strict
peer Router Hash                           = exact
endpoint                                   = 127.0.0.1:<fresh-port>
network ID                                 = exact 99
DeliveryStatus envelope ID                 = exact target ID
DeliveryStatus payload ID                  = exact target ID
observer generation                        = current listener generation only
observer drop count                        = 0 for instrumented pass
receive bounds                             = Plan 093 bounds unchanged
instrumented/control i2pr source           = same exact commit
instrumented/control reference source      = same pinned revision
instrumented/control external behavior     = equivalent
attempt count                              = exactly one instrumented + at most one control
cleanup                                    = clean
Plan 088                                   = blocked until CI pair + status reconciliation
NTCP2                                      = experimental/non-advertised
```

## Work package 1: revalidate current repository state

Before editing workflow code:

1. Record current `main` HEAD.
2. Verify it contains `0e40f403b822907c84acb5ccb669d41d8879ca05` or an equivalent descendant with the Plan 094 implementation surface.
3. Inspect:

```text
plans/094-plan093-completion-and-plan087-to-plan088-handoff.md
plans/087-status.md
plans/088-status.md
README.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
.github/workflows/ntcp2-interop-rootless.yml
tests/integration/ntcp2/harness/plan083_runner.py
tests/integration/ntcp2/harness/minimal_i2pd_probe.py
scripts/interop/run-minimal-i2pd-host-loopback-probe.py
```

4. Confirm no later commit already supplied passing Plan 094 instrumented/control records.
5. Confirm the local environment blocker is not a protocol result.
6. Confirm direct userspace host-loopback is still the intended development topology.

Required implementation note:

```text
current_head
plan094_implementation_commit
local_live_execution = environment_blocked
forward_instrumented_record = absent | nonpassing | passing
forward_control_record = absent | nonpassing | passing
plan093_status = absent | nonpassing | passed
plan087_status = open | passed
plan088_status = blocked | next_executable
```

Acceptance criteria:

- no already-valid passing evidence is duplicated;
- environment limitation is separated from protocol result;
- Plan 095 remains an execution-lane correction, not a protocol redesign.

## Work package 2: create the dedicated CI workflow

Add:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
```

Initial trigger:

```yaml
on:
  workflow_dispatch:
```

Do not enable automatic `pull_request` execution for live interop in this plan.

Top-level requirements:

```yaml
permissions:
  contents: read

concurrency:
  group: ntcp2-host-loopback-development-${{ github.ref }}
  cancel-in-progress: false
```

Recommended jobs:

```text
contract
build
forward-instrumented
forward-control
validate-gate
```

The exact names may vary, but the dependency graph must be equivalent:

```text
contract
   |
   v
build
   |
   v
forward-instrumented
   |
   +---- failure -> workflow stops; diagnostic artifact only
   |
   v
forward-control
   |
   +---- failure -> workflow stops; divergence artifact only
   |
   v
validate-gate
```

Every job must use `ubuntu-24.04` unless a documented repository reason requires another standard GitHub-hosted Ubuntu label.

Acceptance criteria:

- workflow is manual and explicit;
- no privileged networking or namespace operations appear anywhere;
- control cannot execute after a failed instrumented job;
- gate validation cannot execute as passed unless both prior live jobs passed;
- workflow uses only read repository permissions.

## Work package 3: contract job

The contract job validates the entire Plan 094/095 static surface before spending time on reference builds.

Required commands at minimum:

```text
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
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
```

Rootless/Multipass commands here are static repository-boundary checks only. They must not execute either lane.

The contract job must emit a compact sanitized contract record:

```text
schema = i2pr-ntcp2-ci-contract-v1
source_commit
workflow_file_sha256
runner_label
rust_toolchain
plan094_tests_passed
plan095_tests_passed
static_checks_passed
```

Acceptance criteria:

- any required test/check failure blocks build and all live jobs;
- skipped required tests are reported and do not satisfy closure;
- no live router process is started by the contract job.

## Work package 4: deterministic build job

The build job must produce every executable needed by both live jobs from exact immutable inputs.

Build:

```text
i2pr-interop
instrumented i2pd direct driver
control/pristine i2pd direct driver
```

Reference source must remain exactly:

```text
revision = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
```

Required outputs:

```text
target/interop/build/i2pr-build-manifest.json
target/interop/build/i2pd-instrumented-build-manifest.json
target/interop/build/i2pd-control-build-manifest.json
instrumented binary artifact
control binary artifact
i2pr binary artifact
```

Each build manifest must bind:

```text
source_commit or reference_revision
source_tree_sha256
binary_sha256
build command digest or normalized build recipe digest
compiler/toolchain identity
observer patch/header/source digests for instrumented build
control/pristine marker for control build
build timestamp only if excluded from canonical identity comparison
```

The build job must verify:

- instrumented and control reference builds use the same pinned i2pd source revision;
- observer call sites are present only in the instrumented reference build;
- no binary digest is zero;
- no source/build manifest digest is zero;
- `i2pr-interop` corresponds to the checked-out workflow commit;
- all artifacts are regular files and not symlink substitutions.

Upload a named build artifact for downstream jobs. Artifact contents must exclude identities, RouterInfo files, runtime logs, and private run state.

Acceptance criteria:

- downstream live jobs consume exactly the built artifacts or reproduce them byte-for-byte from the same commit;
- build manifests validate before upload;
- instrumented/control provenance is explicit rather than inferred from filenames.

## Work package 5: add Plan 095 workflow-contract tests

Create:

```text
tests/integration/ntcp2/harness/test_plan095.py
```

Required assertions:

1. workflow exists at the exact Plan 095 path;
2. trigger includes `workflow_dispatch`;
3. workflow does not automatically execute live wire tests on `pull_request`;
4. every live job uses `ubuntu-24.04`;
5. permissions are `contents: read` only;
6. prohibited tokens are absent from the workflow live path:

```text
sudo
ip netns
nft
iptables
unshare
--privileged
--network host
multipass
```

7. topology argument is exactly `host-loopback-development`;
8. network ID is exactly `99`;
9. bind address contract is loopback-only;
10. instrumented job depends on build;
11. control job depends on a successful instrumented job;
12. validate-gate depends on successful control;
13. one workflow execution cannot launch a second instrumented attempt automatically;
14. control attempt uses a fresh run root;
15. control attempt uses a fresh target message ID;
16. instrumented/control use the same i2pr source commit;
17. instrumented/control use the same pinned i2pd revision;
18. binary/build/source provenance is required and nonzero for pass;
19. artifacts contain only allowlisted sanitized evidence/build paths;
20. raw run directories are not uploaded;
21. Plan 046 rootless capability is not a prerequisite;
22. Multipass capability is not a prerequisite;
23. live evidence remains development-only/non-release/non-isolation;
24. Plan 088 remains blocked before validated evidence pair;
25. Plan 079 remains blocked;
26. Plan 072 remains inactive;
27. NTCP2 remains experimental/non-advertised.

Tests should parse workflow YAML or use robust structured/line assertions. Do not rely only on vague substring presence.

Acceptance criteria:

- all required environment and sequencing properties are statically enforceable;
- any future accidental reintroduction of rootless/Multipass dependency fails Plan 095 tests.

## Work package 6: instrumented forward job

The instrumented job consumes the validated build artifact and runs exactly one fresh forward attempt.

Required attempt contract:

```text
attempt_kind        = instrumented
attempt_index       = 1
direction           = i2pr-to-i2pd-ipv4
topology_kind       = host-loopback-development
network_id          = 99
bind address        = 127.0.0.1
fresh run root      = yes
fresh state         = yes
fresh identities    = yes
fresh port          = yes
fresh invocation IDs = yes
fresh observer generation = yes
fresh DeliveryStatus ID = yes
```

Use the Plan 094 wrapper/runner and exact provenance inputs. Do not create a second CI-specific protocol oracle.

Required pass predicate:

```text
exact RouterInfo endpoint verified
nonzero i2pr binary/build provenance
nonzero instrumented i2pd binary/build provenance
exact pinned reference source-tree digest
i2pr tcp_connected
i2pd tcp_accepted
i2pr ntcp2_authenticated
i2pd ntcp2_authenticated
bounded valid pre-target traffic accepted
i2pr exact target DeliveryStatus write complete
i2pd exact target DeliveryStatus receive/decode matched
i2pd exact target reply asynchronous write complete after baseline
i2pr exact target reply receive/decode matched
exact peer Router Hash correlation
exact envelope DeliveryStatus ID correlation
exact payload DeliveryStatus ID correlation
current invocation/generation only
observer drop count = 0
i2pr terminal = passed
i2pd terminal = clean
cleanup = clean
all owned processes terminated
record SHA-256 exact nonzero
```

After execution:

1. validate the compact record;
2. calculate its canonical digest;
3. copy only sanitized evidence into an allowlisted evidence directory;
4. delete the raw run root;
5. verify deletion;
6. upload the sanitized instrumented evidence artifact whether the protocol attempt passes or fails, but never upload raw runtime state.

Failure semantics:

- protocol failure => job fails after sanitized artifact upload;
- environment inability to start ordinary loopback processes => emit typed `ci_environment_blocked` diagnostic and fail;
- build/provenance mismatch => fail before live process launch;
- do not retry;
- do not run control;
- do not run Plan 088.

Acceptance criteria:

- exactly one authoritative instrumented attempt occurs per workflow run;
- failure is distinguishable as protocol vs CI-environment vs provenance;
- passing evidence contains no zero provenance and no raw secret material.

## Work package 7: instrumented artifact validation gate

Before control begins, validate the downloaded instrumented artifact independently in the control job or a small intermediate gate.

Required validation:

```text
schema valid
attempt_kind = instrumented
attempt_index = 1
terminal_result = passed
cleanup = clean
all provenance nonzero
source_commit = workflow source commit
reference_revision = pinned revision
exact target DeliveryStatus IDs present
exact peer Router Hash correlation present
observer_drop_count = 0
record digest matches uploaded digest
```

Acceptance criteria:

- GitHub job success alone is not sufficient to authorize control;
- malformed or missing evidence blocks control even if the prior shell command returned zero.

## Work package 8: control forward job

Control runs only after instrumented artifact validation succeeds.

Required attempt contract:

```text
attempt_kind        = control
attempt_index       = 1
direction           = i2pr-to-i2pd-ipv4
topology_kind       = host-loopback-development
network_id          = 99
same i2pr source commit as instrumented
same pinned i2pd revision as instrumented
control/pristine reference build
fresh run root
fresh state
fresh identities
fresh port
fresh invocation IDs
fresh DeliveryStatus ID
same deadline/bound classes
same data-phase oracle configuration
```

The control oracle must not require internal observer events.

Required external success:

```text
i2pr terminal = passed
exact returned DeliveryStatus envelope ID = configured target
exact returned DeliveryStatus payload ID = configured target
expected peer Router Hash = exact
reference process exits cleanly
cleanup = clean
all owned processes terminated
record digest = exact nonzero
all required control provenance = exact nonzero
```

After execution, perform the same sanitation/deletion sequence as instrumented and upload only the sanitized control artifact.

Failure semantics:

- no automatic retry;
- instrumented/control divergence remains a hard blocker;
- Plan 093, Plan 094, and Plan 087 remain open;
- Plan 088 remains blocked.

Acceptance criteria:

- exactly one control attempt occurs after exactly one passing instrumented attempt;
- control success is based on external behavior and provenance, not observer-only data.

## Work package 9: validate instrumented/control equivalence

The final gate job downloads both sanitized evidence artifacts and build manifests.

Validate:

```text
instrumented terminal = passed
control terminal = passed
both cleanup = clean
same i2pr source commit
same i2pr build identity or explicitly equivalent rebuild digest
same pinned i2pd source revision
same reference source-tree digest
instrumented build = observer-enabled
control build = observer-free/pristine
same topology kind
same network ID
same protocol bounds/profile
both exact target exchanges succeeded
both peer Router Hash bindings succeeded
no zero digests
no forbidden raw fields
```

Fresh values that are expected to differ must be explicitly allowlisted:

```text
run_id
port
run root
identity material / RouterInfo digest
DeliveryStatus message ID
invocation IDs
observer generation
runtime timestamps
record digest
```

Generate:

```text
target/interop/evidence/plan095-ci-gate.json
```

Suggested schema:

```text
schema = i2pr-ntcp2-plan095-ci-gate-v1
workflow_run_id
workflow_run_attempt
source_commit
runner_label
runner_arch
reference_revision
contract_record_sha256
build_artifact_manifest_sha256
forward_instrumented_record_sha256
forward_control_record_sha256
instrumented_result = passed
control_result = passed
equivalence = passed
cleanup = clean
plan087_gate = satisfied
plan088_gate = ready_after_status_reconciliation
```

The gate record must not contain credentials, runner tokens, private runtime paths, raw logs, or raw RouterInfo bytes.

Acceptance criteria:

- the gate job fails if either evidence record is absent, malformed, zero-provenance, or nonpassing;
- workflow run success therefore means the complete forward evidence pair exists and validates.

## Work package 10: artifact retention and sanitation

Upload only:

```text
target/interop/evidence/*.json
target/interop/build/*.json
```

or a narrower explicit allowlist if those directories may contain other material.

Never upload:

```text
target/interop/runs/**
raw/**
state/**
i2pd-data/**
exchange/**
router.info
*.key
*.dat identity files
pcap/capture files
unredacted logs
socket dumps
ciphertext/plaintext transcripts
```

Artifact retention may use a short bounded period such as 14 days, matching the existing NTCP2 manual workflow convention. The durable repository record should be cryptographic digests and sanitized status metadata, not permanent raw CI artifacts.

Acceptance criteria:

- raw run roots are deleted before artifact upload;
- artifact paths are statically allowlisted;
- evidence validator rejects forbidden fields and private paths.

## Work package 11: CI environment blocker semantics

The hosted runner is expected to support ordinary local processes and loopback sockets, but Plan 095 must still fail closed if it does not.

Define a bounded CI environment blocker vocabulary, for example:

```text
ci_binary_execution_blocked
ci_loopback_bind_blocked
ci_loopback_connect_blocked
ci_reference_build_blocked
ci_artifact_transfer_blocked
ci_disk_space_blocked
ci_unexpected_runner_environment
```

Do **not** map protocol errors into these codes.

A CI environment blocker record must include only sanitized metadata:

```text
source_commit
runner_label
runner_arch
workflow_run_id
stage
reason_code
relevant command class
cleanup result
record digest
```

If GitHub-hosted `ubuntu-24.04` cannot run the ordinary host-loopback lane:

1. preserve one typed sanitized blocker;
2. stop the workflow;
3. do not automatically switch to rootless, Multipass, Docker, or self-hosted;
4. create a separate successor plan evaluating a labeled self-hosted Ubuntu runner or another explicit environment.

Acceptance criteria:

- CI inability is not misreported as NTCP2 failure;
- there is no silent environment fallback.

## Work package 12: status authority before first CI execution

After workflow implementation/tests land but before a successful live CI run, update current status authority to:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-closure-next-executable
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

Update current authority in:

```text
plans/087-status.md
plans/088-status.md
README.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

Do not create `plans/093-status.md` with `passed` yet.

Acceptance criteria:

- no current authority says rootless or Multipass is required for Plan 095;
- Plan 088 remains blocked until successful CI evidence and later status reconciliation.

## Work package 13: commit workflow implementation before executing it

Required implementation commit contains:

```text
new Plan 095 workflow
Plan 095 tests
any minimal wrapper/runner CI parameterization
artifact/evidence validator updates
status authority naming Plan 095 next executable
static boundary enforcement
```

Before commit:

```text
all Plan 095 focused tests pass
all Plan 094/093 regressions pass
static boundary checks pass
cargo checks/tests pass
workflow syntax validates
git diff --check passes
```

The workflow must execute from this exact clean commit or a later commit containing only documentation/status corrections that do not alter runtime behavior.

Acceptance criteria:

- live CI evidence never comes from an uncommitted or locally modified tree;
- the exact workflow file digest is recorded in CI evidence.

## Work package 14: execute one manual Plan 095 CI run

Trigger exactly one workflow run for the candidate closure commit.

Required inputs should be minimal. Preferred input set:

```text
source_ref = default current branch/ref implicit in workflow dispatch
```

Avoid user-provided arbitrary shell fragments, arbitrary binary paths, arbitrary commands, or arbitrary run-root paths.

Execution order:

```text
contract
build
forward-instrumented
forward-control
validate-gate
```

Stop at first failed gate.

Do not rerun the workflow automatically after a failure. A manual second workflow execution is a new evidence cycle and requires reviewing the first failure reason before execution.

Acceptance criteria:

- one workflow run maps to one bounded evidence cycle;
- workflow run ID and run attempt are captured in the gate record;
- a successful workflow run means all five jobs and both evidence records passed.

## Work package 15: reconcile Plan 093/094/087/088 after successful CI closure

Only after the Plan 095 gate record validates as passed:

### Create `plans/093-status.md`

Required current tokens:

```text
status = passed
classification = post-auth-data-phase-sequencing-corrected
completion_plan = plan095
correction_commit = <exact source commit>
forward_instrumented_record_sha256 = <exact 64-hex>
forward_control_record_sha256 = <exact 64-hex>
reference_revision = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
ci_gate_record_sha256 = <exact 64-hex>
cleanup = clean
```

### Record Plan 094 completion

Plan 094 may be marked completed-by-Plan-095 rather than pretending its local execution path succeeded:

```text
status = passed-via-plan095-ci-lane
local_execution = environment_blocked
ci_execution = passed
```

### Rewrite `plans/087-status.md`

```text
status = passed
plan_093 = passed
plan_094 = passed-via-plan095-ci-lane
plan_095 = passed
forward_instrumented_record_sha256 = <exact 64-hex>
forward_control_record_sha256 = <exact 64-hex>
ci_gate_record_sha256 = <exact 64-hex>
```

### Rewrite `plans/088-status.md`

```text
decision = not-yet-run
plan_087 = passed
plan_093 = passed
plan_094 = passed-via-plan095-ci-lane
plan_095 = passed
plan_088 = next_executable
```

Update README, AGENTS, and interop skill so Plan 088 is the only next executable action.

Keep:

```text
plan_079 = blocked_pending_plan_088_two_way_pass
plan_072 = inactive_pending_plan_088_ambiguity
ntcp2 = experimental_non_advertised
```

Acceptance criteria:

- local environment blocker remains historically accurate;
- CI evidence, not prose, closes the forward direction;
- Plan 088 is next-executable but has no fabricated reverse result.

## Work package 16: prepare Plan 088 to reuse the CI lane

Do not execute Plan 088 in Plan 095, but make the handoff explicit:

Plan 088 should inherit:

```text
runner = github-hosted ubuntu-24.04
topology_kind = host-loopback-development
development_only = true
network_id = 99
loopback-only endpoints
same deterministic build/provenance contract
instrumented-then-control sequencing
sanitized artifact policy
no rootless/Multipass prerequisite
```

The reverse direction remains:

```text
i2pd-to-i2pr-ipv4
```

Plan 088 may require its own workflow mode/job sequence or a successor workflow extension, but that work is out of scope for Plan 095 except for ensuring the forward artifacts and build machinery are reusable.

Acceptance criteria:

- Plan 088 does not regress to the unavailable local/rootless/Multipass prerequisite;
- Plan 088 remains a separate execution/decision phase.

## Required validation commands

At minimum before committing the Plan 095 implementation:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan095.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
git diff --check
```

If a workflow linter already exists in the repository, run it. Do not add a large workflow-tooling dependency solely for YAML linting; focused Python/static assertions are sufficient for this scope.

## Explicit Plan 095 closure criteria

Plan 095 closes only when every item is true:

- [ ] Current repository state was revalidated before implementation.
- [ ] The local live-wire limitation is recorded as an environment blocker, not a protocol failure.
- [ ] A dedicated manual GitHub Actions host-loopback-development workflow exists.
- [ ] The workflow primary runner is standard `ubuntu-24.04`.
- [ ] Workflow permissions are read-only for repository contents.
- [ ] The workflow contains no sudo, userns/unshare, namespace, firewall mutation, Multipass, privileged-container, or host-network dependency.
- [ ] The workflow is development-only and cannot produce release/isolation qualification.
- [ ] Contract checks run before reference build/live execution.
- [ ] Plan 095 focused tests enforce workflow sequencing and prohibited dependencies.
- [ ] i2pr, instrumented i2pd, and control i2pd builds are provenance-bound.
- [ ] Instrumented/control reference builds use the exact same pinned i2pd revision.
- [ ] Attempted-live binary/build/source/scenario/placement digests are exact and nonzero.
- [ ] Exactly one instrumented attempt executes in one evidence cycle.
- [ ] Control cannot execute unless instrumented evidence independently validates as passed.
- [ ] Exactly one control attempt executes after the instrumented pass.
- [ ] Instrumented pass proves exact bidirectional DeliveryStatus exchange, both-sided authentication, exact peer Router Hash, current observer generation, zero observer drops, and clean teardown.
- [ ] Control pass proves equivalent external DeliveryStatus success without observer-only evidence.
- [ ] Instrumented/control use the same i2pr source commit and same pinned i2pd revision.
- [ ] Instrumented/control evidence records have exact nonzero SHA-256 digests.
- [ ] A Plan 095 CI gate record binds workflow run metadata to both evidence digests.
- [ ] Raw run roots and secret-bearing state are deleted before artifact upload.
- [ ] Uploaded artifacts contain only sanitized evidence and build manifests.
- [ ] A CI environment failure produces a typed environment blocker and does not trigger fallback or retry.
- [ ] `plans/093-status.md` records `status = passed` only after the CI evidence pair passes.
- [ ] Plan 094 is recorded as completed via the Plan 095 CI lane without falsifying the local environment history.
- [ ] `plans/087-status.md` records `status = passed` with both exact forward digests.
- [ ] `plans/088-status.md` records `decision = not-yet-run` and `plan_088 = next_executable`.
- [ ] Plan 079 remains blocked pending the actual Plan 088 two-way result.
- [ ] Plan 072 remains inactive pending the actual Plan 088 ambiguity result.
- [ ] NTCP2 remains experimental, disabled by default, and non-advertised.

If any checkbox is false, Plan 088 remains blocked.

## Failure ownership and stop conditions

### Contract/build fails

Fix only the bounded test/build/workflow surface. Do not alter NTCP2 behavior unless a deterministic protocol regression proves it is necessary.

### GitHub-hosted CI cannot run ordinary loopback processes

Record one typed `ci_environment_blocked` artifact and stop. Do not switch lanes automatically. A successor plan may evaluate an explicitly labeled self-hosted Ubuntu runner.

### Instrumented fails before authentication

Retain sanitized evidence and classify using the existing metadata-only handshake observer. Do not run control. Create a new narrow protocol/harness plan.

### Instrumented authenticates but target exchange fails

Use exact Plan 093 oracle/observer reasons. Do not retry or increase deadlines. Do not run control. Create a narrow successor.

### Instrumented passes but control fails

Treat observer/control parity as the blocker. Do not close Plan 093/087 and do not run Plan 088.

### Both pass but artifact/status sanitation fails

Do not rerun wire attempts solely for documentation/index formatting. Correct only the sanitation/status layer if validated compact records and digests remain intact and no runtime/harness behavior changes.

### Both pass and gate record validates

Close Plan 093, mark Plan 094 completed via Plan 095, close Plan 087, mark Plan 088 next-executable, and stop. Plan 088 executes separately.

## Smaller-model execution sequence

Execute in this exact order and stop on the first failed gate:

1. Read Plans 093, 094, 087 status, and 088 status.
2. Record current HEAD and verify Plan 094 implementation is present.
3. Verify no valid passing forward evidence pair already exists.
4. Record local execution as environment-blocked, not protocol-failed.
5. Add `.github/workflows/ntcp2-interop-host-loopback-development.yml`.
6. Add `test_plan095.py` workflow-contract tests.
7. Add only minimal runner/wrapper options required for CI invocation.
8. Add/extend sanitized CI contract and gate-record schemas.
9. Add build artifact/manifests required by downstream jobs.
10. Make the workflow contract job run Plan 095/094/093/083/i2pd/088 checks.
11. Make the build job produce all three exact binaries plus manifests.
12. Make the instrumented job run exactly one host-loopback-development attempt.
13. Make instrumented sanitation/upload run even on failure without uploading raw state.
14. Make control depend on successful independent validation of instrumented evidence.
15. Make control run exactly one fresh attempt.
16. Add final equivalence/gate validation job.
17. Enforce no rootless, Multipass, namespace, firewall, Docker, or privileged dependency.
18. Update pre-live status authority to Plan 095 as next executable.
19. Run all required local static/unit validation.
20. Commit the Plan 095 implementation.
21. Dispatch exactly one manual workflow run for that commit.
22. If contract/build fails, stop and correct before another evidence cycle.
23. If CI environment is blocked, retain typed blocker and stop.
24. If instrumented fails, retain sanitized record and stop.
25. If instrumented passes, independently validate its artifact.
26. Run exactly one control attempt.
27. If control fails/diverges, retain sanitized record and stop.
28. Validate both records, manifests, and equivalence.
29. Generate and validate the Plan 095 CI gate record.
30. Download/record exact artifact/evidence digests for durable status.
31. Create `plans/093-status.md` as passed.
32. Mark Plan 094 completed via Plan 095 CI lane.
33. Rewrite Plan 087 status as passed.
34. Rewrite Plan 088 status as not-yet-run / next-executable.
35. Update README, AGENTS, and interop skill to Plan 088 as the only next action.
36. Keep Plan 079 blocked and Plan 072 inactive.
37. Rerun focused status/static validation.
38. Commit closure/status reconciliation.
39. Report Plan 088 ready for separate CI-backed execution.

## Executor final report requirements

The handoff executor must report:

```text
Plan 095 implementation commit
Plan 095 closure/status commit
GitHub Actions workflow path
workflow run ID
workflow run attempt
runner label
runner architecture
source commit
reference revision
contract job result
build job result
instrumented job result
control job result
validate-gate result
instrumented run ID
instrumented record SHA-256
instrumented i2pr binary SHA-256
instrumented i2pd binary SHA-256
instrumented build-manifest SHA-256 values
instrumented terminal result
instrumented cleanup result
control run ID
control record SHA-256
control i2pr binary SHA-256
control i2pd binary SHA-256
control build-manifest SHA-256 values
control terminal result
control cleanup result
Plan 095 CI gate record SHA-256
whether raw run state was deleted
whether only sanitized artifacts were uploaded
Plan 093 final status
Plan 094 final status
Plan 087 final status
Plan 088 gate status
Plan 079 gate status
Plan 072 gate status
NTCP2 default-enable/advertisement status
```

A report containing only GitHub Actions job success without the exact evidence/provenance/gate values is insufficient.

## Handoff state before execution

```text
plan_086 = host-loopback-development-ready
plan_090 = routerinfo-correction-landed
plan_091 = historical-partial-correction
plan_092 = superseded-by-plan093
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = planned-next-executable-ci-live-wire-closure
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

## Handoff state after successful execution

```text
plan_093 = passed
plan_094 = passed-via-plan095-ci-lane
plan_095 = passed
plan_087 = passed
plan_088 = next-executable-not-yet-run
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```
