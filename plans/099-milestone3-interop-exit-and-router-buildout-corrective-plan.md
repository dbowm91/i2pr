# Plan 099: Milestone 3 interoperability exit, harness reduction, and router-buildout corrective plan

## Status and authority

- Status: planned; active corrective/exit plan.
- Date: 2026-08-11.
- Planning baseline: `45ee8b3a08287deb833370218d9c43b19d4e22ad` or a clean descendant that has not materially changed the NTCP2 development-interoperability architecture.
- Parent authority: Plan 067 staged interoperability roadmap and ADR 0023.
- Immediate predecessors: Plans 095–098.
- Primary independent validator: pinned i2pd 2.60.0 at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Development topology: `host-loopback-development`, literal IPv4 `127.0.0.1`, private network ID 99.
- NTCP2 remains experimental, non-advertised, and disabled in normal daemon operation throughout this plan.
- This plan is intentionally an **exit plan** from the current interoperability-artifact expansion. It is not another evidence-framework plan.

Plan 099 supersedes the active execution interpretation of the Plan 095–098 multi-job CI/provenance sequence for **development interoperability**. Historical plan documents remain audit history, but their plan-number-specific workflow/test machinery is no longer entitled to grow indefinitely or gate unrelated router construction.

This plan also narrows the Plan 079 continuation interpretation. Repeated Level 2 validation remains useful before normal NTCP2 activation and before depending on NTCP2 for public-network router operation, but it is no longer permitted to block production-daemon composition, local RouterInfo publication architecture, local NetDB storage/state machines, reseed parsing, or other offline/local Milestone 4 implementation work.

## Why this plan exists

The repository is a Rust I2P router project, but development effort has become disproportionately concentrated in Python interoperability orchestration, plan-specific schema validators, static grep checks, status-token propagation, CI artifact transfer, and provenance manifests.

That work solved real problems at first: the project needed to distinguish self-consistent NTCP2 code from independently interoperable NTCP2 behavior. However, the marginal value of additional layers has fallen sharply.

The current repository already has:

- a substantial Rust NTCP2 implementation;
- runtime-owned TCP/link behavior;
- a real pinned i2pd 2.60.0 direct driver;
- a passive instrumented i2pd observer and uninstrumented control build;
- forward and reverse real-subprocess runner logic;
- exact Router Hash and DeliveryStatus correlation support;
- a GitHub-hosted Ubuntu 24.04 environment that can build i2pd and i2pr and run host-loopback development probes;
- accepted staged-evidence policy in ADR 0023 that explicitly separates development evidence from release qualification.

At the same time, the production router remains much less developed:

- `i2pr-daemon run` still returns `RuntimeNotImplemented` for non-dry-run execution;
- no production router composition owns long-lived transport/NetDB services;
- NetDB storage, lookup, publication, and reseed behavior remain unimplemented;
- tunnels, garlic routing, streaming, SAM, SSU2, and I2CP remain future milestones.

Plan 099 corrects the allocation of engineering effort.

## External protocol research and interoperability value

### Sources reviewed

Authoritative technical sources for this plan:

1. I2P NTCP2 transport specification, current documentation snapshot updated 2026-03 and accurate for I2P API 0.9.69:
   - https://i2p.net/en/docs/specs/ntcp2/
2. I2P transport overview:
   - https://i2p.net/en/docs/overview/transport/
3. I2P I2NP specification:
   - https://i2p.net/en/docs/specs/i2np/
4. Repository-pinned i2pd implementation and source lock:
   - https://github.com/PurpleI2P/i2pd
   - pinned revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`
5. Existing project authority:
   - `docs/adr/0023-staged-ntcp2-interoperability-evidence.md`
   - `specs/protocols/03-ntcp2.md`
   - `specs/protocols/02-i2np.md`

### What independent NTCP2 interoperability actually proves

The official NTCP2 specification defines NTCP2 as an authenticated point-to-point router transport for carrying I2NP messages. The establishment sequence is:

```text
Alice / initiator             Bob / responder

SessionRequest  ------------------>
                <------------------ SessionCreated
SessionConfirmed ----------------->

then authenticated Data messages in either direction
```

The highest-value independent test is therefore not a large evidence hierarchy. It is a real second implementation proving that i2pr interprets the same wire protocol sufficiently to:

1. consume the peer's RouterInfo/NTCP2 static-key material correctly;
2. complete the NTCP2 Noise XK-based establishment as initiator;
3. complete the same establishment as responder;
4. derive compatible authenticated data-phase state;
5. encode/decode at least one real I2NP message across that authenticated channel;
6. preserve exact identity and message correlation rather than accepting a generic successful socket exchange.

For the current milestone, `DeliveryStatus` is a good minimal I2NP message because it is small, deterministic, already implemented in i2pr, and can be correlated by exact nonzero message ID. It is not intended to prove NetDB, tunnels, garlic routing, or application traffic.

### What it does not prove

A passing host-loopback NTCP2 interoperability smoke does **not** prove:

- release-grade isolation;
- public-network safety;
- anonymity properties;
- resistance to global observation;
- Java I2P compatibility;
- SSU2 compatibility;
- NetDB correctness;
- tunnel correctness;
- production resource behavior;
- long-duration stability;
- default-enable readiness.

Those are separate questions and must not be smuggled into this development gate.

### Diminishing returns boundary

After one authenticated, correlated, bidirectional i2pr/i2pd result plus an uninstrumented control comparison, more plan-number-specific schemas, manifests, status tokens, workflow handoffs, and static string assertions provide much less protocol confidence than implementing the next router subsystem.

Therefore Plan 099 establishes this development-value threshold:

```text
minimum valuable independent NTCP2 evidence
    = one fresh-state instrumented pass i2pr -> i2pd
    + one behavior-neutral control pass i2pr -> i2pd
    + one fresh-state instrumented pass i2pd -> i2pr
    + one behavior-neutral control pass i2pd -> i2pr
    + exact Router Hash correlation
    + exact DeliveryStatus correlation
    + clean process/state teardown
```

This threshold is sufficient to stop treating basic NTCP2 wire compatibility as a global blocker for router construction.

Repeated 3/3 Level 2 development validation remains valuable later, but it is deferred to the integration checkpoint before normal-daemon NTCP2 activation/public-network dependence. It is not a prerequisite for offline/local Milestone 4 implementation.

## Environment conclusion

### Known local environment constraints

The constrained development host cannot reliably provide the historical release-grade execution environments:

- unprivileged user namespaces are blocked by the host/AppArmor configuration;
- Multipass has been unavailable, stale, or too resource-constrained for repeatable use;
- those limitations make the old rootless/guest evidence lanes unsuitable as the primary iterative development path.

Plan 099 must not spend additional effort repairing those lanes.

### Working environment available now

The repository has already demonstrated that GitHub-hosted `ubuntu-24.04` can:

- install the required build packages;
- build the pinned i2pd source and both direct-driver variants;
- build `i2pr-interop`;
- execute a host-loopback live job;
- reach the current real-subprocess runner.

The remaining development environment should therefore be deliberately simple:

```text
one fresh GitHub Actions ubuntu-24.04 job
one checked-out i2pr source commit
one pinned i2pd source tree
one in-place i2pr-interop release binary
one in-place instrumented i2pd driver
one in-place control i2pd driver
literal 127.0.0.1 only
network ID 99
fresh state directory per attempt
fresh ports per attempt
no artifact transfer between jobs
no rootless namespace
no Multipass
no Docker
no public I2P bootstrap
```

The current Plan 095 architecture separates build, instrumented, control, and validation into multiple jobs and then re-establishes file identity through artifact upload/download. That architecture caused a long sequence of failures involving artifact paths, executable bits, build manifests, transferred files, provenance rebinding, and workflow-specific state.

For development interoperability, those failures are self-inflicted complexity. Build and execute in the same fresh CI job.

## Current Plan 098 residual defects

Before the simplified live execution is authoritative, Plan 099 owns four small evidence-integrity corrections discovered after Plan 098 landed.

### D1 — chained inequality in final gate

Current form is semantically wrong:

```python
instrumented_source != control_source != expected_source
```

and similarly for the reference revision.

This does not mean "all three are not equal" and can allow two records carrying the same wrong value to pass.

Required form is positive equality or its explicit negation:

```python
not (instrumented_source == control_source == expected_source)
```

or equivalent explicit comparisons.

### D2 — canonical i2pd tracked-tree digest is encoded two different ways

The workflow currently aggregates ASCII hexadecimal child digests, while the Python/build-driver implementation aggregates raw 32-byte child digests. The Python and shell fallback branches of `build-driver.sh` also differ.

Required correction:

- choose one canonical algorithm;
- define it once in documentation/tests;
- use the same byte representation in workflow/build-driver/wrapper if those surfaces remain;
- or remove the redundant tree digest entirely from development smoke if same-job pinned revision + clean tracked tree + exact binary digest already supplies the required development identity.

Plan 099 should prefer deletion of redundant provenance over maintaining three independent implementations of the same digest algorithm.

### D3 — control record does not receive the same exact i2pr provenance validation

The current final gate validates the instrumented record's i2pr binary/build-manifest claims against the real downloaded artifact but does not equivalently bind the control record.

Required correction if manifests remain:

```text
instrumented.i2pr_binary_sha256 == actual_i2pr_binary_sha256
control.i2pr_binary_sha256      == actual_i2pr_binary_sha256
instrumented.i2pr_manifest      == actual_i2pr_manifest
control.i2pr_manifest           == actual_i2pr_manifest
```

If the same-job simplification deletes the development-only i2pr manifest, both records must still carry the same measured binary digest and source commit.

### D4 — preflight still permits an omitted authoritative i2pr binary

The wrapper's preflight path can currently substitute `/nonexistent` when `--i2pr-binary` is absent.

Required correction:

- all attempted-live preflight/forward/reverse paths require the explicit executable path;
- no reconstructed/default/fallback path is permitted;
- purely synthetic/unit tests may inject fake paths through direct runner APIs, not through the attempted-live CLI contract.

## Plan objectives

Plan 099 has seven objectives, in order:

1. **Freeze interoperability architecture growth.** No additional plan-number-specific NTCP2 evidence framework may be added.
2. **Fix the four residual Plan 098 evidence-integrity defects.** Keep this patch small.
3. **Collapse development CI to one build-and-run job.** Eliminate cross-job binary artifact transfer and most development-only manifest machinery.
4. **Run the smallest valuable independent two-way interoperability matrix.** Instrumented + control, both directions, exact identity/message correlation.
5. **Prune superseded Python/static-check machinery.** Preserve protocol-level functional coverage while removing plan-history execution scaffolding from the active tree/default CI.
6. **Amend continuation authority.** Router composition and local/offline Milestone 4 work may proceed even if a remaining NTCP2 wire defect is localized; only transport activation/public-network dependence remains gated.
7. **Hand off to actual router construction.** The next substantial plan after 099 must be production daemon composition + RouterInfo/NetDB foundation, not another generalized NTCP2 harness plan.

## Hard scope lock

Plan 099 may modify:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
scripts/interop/run-minimal-i2pd-host-loopback-probe.py
tests/integration/ntcp2/harness/plan083_runner.py
tests/integration/ntcp2/harness/plan084_runner.py
tests/integration/ntcp2/harness/preflight_runner.py
tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh
focused NTCP2 development-interoperability tests
scripts/check-plan095-workflow.sh
scripts/check-ntcp2-interoperability.sh
plans/087-status.md
plans/088-status.md
plans/030-milestone-3-closure.md
docs/adr/0023-staged-ntcp2-interoperability-evidence.md
README.md
AGENTS.md
docs/protocol-support.md
docs/architecture/interop-apparatus.md
.opencode/skills/i2pr-ntcp2-interop/*
```

It may delete superseded plan-specific Python tests/checker logic after unique functional invariants have been migrated to a small shared test surface.

It must **not** modify NTCP2 crypto/state-machine/frame behavior unless the simplified real wire run reaches TCP and produces a reproducible protocol failure that directly identifies an i2pr-owned protocol defect.

It must not add NetDB/tunnel/SAM/SSU2 production implementation inside Plan 099. This plan only clears the path and defines the handoff.

## Non-goals

Plan 099 must not:

- repair or retry the Plan 046 rootless lane;
- repair or retry Multipass;
- create a new VM/container lane;
- use the public I2P network;
- use reseed;
- introduce Docker;
- add Java I2P as a development prerequisite;
- add Emissary unless a later, precise protocol ambiguity requires a differential;
- add recurring CI;
- add a new release-certificate format;
- add another plan-specific probe schema;
- add another plan-specific status vocabulary;
- add another build-manifest layer for development smoke;
- add packet capture;
- add raw key/transcript/ciphertext retention;
- add generalized retry logic;
- increase timeouts merely to force a pass;
- enable or advertise NTCP2 in `i2pr-daemon`;
- require Level 3 qualification before continuing local router construction.

## Work package 1 — baseline and complexity inventory

Before editing, record a compact baseline in the implementation commit message or Plan 099 status section.

Measure tracked source lines, at minimum:

```bash
git ls-files '*.rs' | xargs wc -l
git ls-files '*.py' | xargs wc -l
git ls-files 'tests/integration/ntcp2/harness/*.py' | xargs wc -l
```

Also count:

```bash
git ls-files 'tests/integration/ntcp2/harness/test_plan*.py' | wc -l
git ls-files 'plans/*.md' | wc -l
```

The purpose is not to optimize a vanity metric. It is to prove that Plan 099 reduces active harness burden rather than adding another layer.

Record the top five largest tracked Python files under the NTCP2 harness by line count.

No new measurement framework or dependency is required. `wc -l` is sufficient.

## Work package 2 — fix only the four residual evidence defects

Implement D1–D4 above.

Focused regression requirements:

1. same wrong source commit in instrumented/control must fail the gate;
2. same wrong reference revision in instrumented/control must fail the gate;
3. instrumented correct + control wrong i2pr digest must fail;
4. control correct + instrumented wrong i2pr digest must fail;
5. the chosen source-tree identity algorithm must produce one identical value from every remaining implementation;
6. attempted-live preflight without explicit `--i2pr-binary` must fail before subprocess launch;
7. explicit arbitrary absolute i2pr path must work without `target/debug` or `target/release` fallback.

Do not create `test_plan099.py` solely to assert strings in documentation/workflow text. Prefer a shared functional test name such as:

```text
test_ntcp2_development_interop.py
```

or merge these cases into an existing functional runner test.

## Work package 3 — collapse Plan 095 development CI into one job

Reuse the existing workflow filename to avoid another workflow artifact:

```text
.github/workflows/ntcp2-interop-host-loopback-development.yml
```

Replace the current multi-job build/artifact-transfer graph with one manually dispatched job.

Required workflow shape:

```text
workflow_dispatch
  -> development-interop (ubuntu-24.04)
       -> checkout exact source commit
       -> install bounded build dependencies
       -> install pinned Rust toolchain/components
       -> fetch/checkout exact pinned i2pd revision
       -> assert pinned revision and tracked-tree cleanliness
       -> build i2pd libraries/drivers with bounded parallelism
       -> build i2pr-interop release binary
       -> assert executable paths
       -> run focused local contract tests
       -> forward instrumented fresh-state attempt
       -> forward control fresh-state attempt, only if instrumented passes
       -> reverse instrumented fresh-state attempt, only if forward pair passes
       -> reverse control fresh-state attempt, only if reverse instrumented passes
       -> validate one compact sanitized summary
       -> upload only the sanitized summary/records required for development evidence
```

### Required simplifications

- No `actions/upload-artifact` / `actions/download-artifact` cycle for binaries between jobs.
- No chmod restoration caused by archive transfer.
- No build-output path rebinding between jobs.
- No separate contract/build/instrumented/control/validate jobs.
- No GitHub artifact used as the authoritative binary transport.
- No per-job source-tree reconstruction.
- No duplicated source/build manifests unless a field is genuinely necessary for the development result.
- No status-token mutation during the run.

The job may upload the final sanitized JSON evidence after all processes have stopped and raw state has been removed.

### Build constraints

- `runs-on: ubuntu-24.04`.
- `permissions: contents: read`.
- i2pd revision exactly `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- i2pd build parallelism bounded to `--parallel 2` unless measured CI memory data justifies another fixed value.
- Rust toolchain remains pinned to the repository's normal development pin.
- No sudo in live probe execution. Package installation before build may use the normal GitHub-hosted runner package mechanism.

## Work package 4 — minimal two-way development matrix

### Common constraints

Every attempt must use:

```text
topology_kind = host-loopback-development
development_only = true
release_qualified = false
isolation_qualified = false
bind_address = 127.0.0.1
peer_address = 127.0.0.1
network_id = 99
reference = i2pd 2.60.0
fresh mutable state
fresh run id
fresh ports
nonzero unique DeliveryStatus message id
bounded handshake/data deadlines
```

No run may reseed, bootstrap NetDB, invoke SAM/I2CP/HTTP/SOCKS, start SSU2, or activate normal `i2pr-daemon` networking.

### M1 — forward instrumented

Run:

```text
i2pr initiator -> i2pd responder
```

Pass requires authentic current-run observations proving:

```text
tcp_connected
ntcp2_authenticated/session established
one authenticated data frame carrying the target I2NP path
exact DeliveryStatus message ID observed by the independent reference
exact expected i2pr/i2pd Router Hash binding
clean teardown
```

Do not require unrelated NetDB/tunnel behavior.

### M2 — forward control

Use the uninstrumented i2pd control binary through the same real transport path.

Pass requires the i2pr-facing protocol result to remain successful with the observer disabled. The control does not need to reproduce observer-only diagnostic fields that by definition do not exist in the control build.

The control exists only to show observer instrumentation is not the reason the protocol path works.

### M3 — reverse instrumented

Run:

```text
i2pd initiator -> i2pr responder
```

Require the same authentication, exact identity binding, one exact correlated DeliveryStatus, and cleanup invariants.

### M4 — reverse control

Repeat the reverse direction through the uninstrumented control build and require the same protocol outcome.

### Development interop result

The compact summary has exactly these terminal states:

```text
passed
protocol-defect-localized
environment-or-harness-blocked
```

Do not introduce another large reason vocabulary.

`passed` requires M1–M4 pass from the same source commit and pinned i2pd revision.

`protocol-defect-localized` requires a real current-run TCP/NTCP2 stage plus one specific reproducible i2pr-owned incompatibility.

`environment-or-harness-blocked` is only valid before authentic TCP/protocol evidence.

## Work package 5 — bounded failure policy and hard stop

This is the most important anti-overengineering rule in Plan 099.

### Pre-TCP failure

If the simplified same-job workflow fails before authentic TCP:

1. preserve a compact sanitized reason;
2. inspect only the failing command/path/environment assumption;
3. make at most one narrow harness correction;
4. rerun once.

Do not create a new plan, schema, manifest, or topology for that correction.

### Post-TCP protocol failure

If the run reaches authentic TCP and fails during NTCP2 establishment/data phase:

1. preserve the highest real stage and bounded events;
2. reproduce once from fresh state without changing timeouts/binaries;
3. inspect the official NTCP2 section and pinned i2pd owner for that exact stage;
4. if ownership is clearly i2pr, make one narrow Rust protocol/runtime correction;
5. rerun the affected direction from fresh state;
6. do not broaden the harness.

### After one bounded correction

If the direction still fails after one owned correction:

- record `protocol-defect-localized` with the exact stage/question;
- keep NTCP2 disabled/non-advertised;
- do **not** add another generalized NTCP2 corrective plan immediately;
- continue production daemon composition and local/offline NetDB work;
- return to the localized transport defect when the next network-integration checkpoint requires it.

This rule is deliberate. A transport defect may block using NTCP2 for public router networking, but it does not block implementing the router supervisor, persistent local state, RouterInfo publication ownership, NetDB storage/indexing, SU3 parsing, lookup state machines under deterministic tests, or tunnel-independent local architecture.

## Work package 6 — Python/harness reduction

Plan 099 must produce a **net reduction** in active interoperability machinery.

### 6.1 Stop plan-number-specific Python growth

After this plan:

- no new `test_planNNN.py` may be added for NTCP2 development interoperability;
- no new plan-number-specific Python runner may be added;
- no static check may require documentation status tokens merely to make protocol tests pass;
- historical plan documents remain readable but are not executable API contracts.

### 6.2 Consolidate functional coverage

Migrate unique, still-valuable functional assertions from Plan 090–098 plan-specific Python tests into a small shared set organized by behavior, for example:

```text
test_ntcp2_development_interop.py
test_i2pd_direct_driver.py
test_minimal_i2pd_probe.py
```

Names are illustrative; do not create files when an existing functional file is a better owner.

Delete superseded plan-specific Python tests after their unique functional assertions are migrated. Git history is the historical archive.

### 6.3 Remove redundant static workflow checker

Once the workflow is one job and functional tests cover its critical inputs, remove `scripts/check-plan095-workflow.sh` unless it still protects a small invariant that cannot reasonably be tested functionally.

If any logic remains, fold it into the general interoperability boundary checker and keep it protocol-oriented, not plan-token-oriented.

### 6.4 Trim `check-ntcp2-interoperability.sh`

The active checker should enforce only durable invariants such as:

- NTCP2 remains experimental/non-advertised;
- production daemon does not accidentally activate it;
- direct reference driver is test-only;
- no public-network/reseed/SAM/I2CP fallback in the development smoke;
- pinned reference revision exists;
- functional interop tests exist;
- lower-tier evidence cannot satisfy release qualification.

Remove checks whose sole purpose is proving that old plan prose, old status tokens, or superseded test filenames exist.

### 6.5 Quantitative reduction acceptance

Re-run the WP1 LOC measurements after cleanup.

Required result:

- tracked Python LOC under `tests/integration/ntcp2/harness/` plus `scripts/interop/` is reduced by **at least 40%** from the Plan 099 baseline **or** all Plan 090–098 plan-number-specific Python test/runner/checker code is removed and the remaining Python is explicitly justified as reusable functional interop infrastructure;
- total tracked Python LOC must not increase in this plan;
- Rust production/router LOC must not be deleted to improve the ratio;
- protocol fixtures are not deleted merely to improve LOC counts.

The semantic alternative exists to avoid reckless deletion if shared Python below the 40% threshold is genuinely reusable.

### 6.6 CI runtime reduction

The default/manual development interop path must not run the full historical 1000+ case plan matrix.

Run only:

- focused retained functional interop tests;
- relevant Rust workspace tests;
- durable dependency/runtime boundary checks.

Historical matrices may be invoked manually from git history if forensic archaeology is ever required.

## Work package 7 — simplify durable evidence

Development evidence should be one compact JSON summary plus, when useful, one small sanitized record per direction/role.

The summary should bind only values needed to answer the development question:

```text
schema
source_commit
reference_revision
topology_kind
development_only
release_qualified
isolation_qualified
i2pr_binary_sha256
i2pd_instrumented_binary_sha256
i2pd_control_binary_sha256
forward_instrumented_result
forward_control_result
reverse_instrumented_result
reverse_control_result
forward_router_hash_match
reverse_router_hash_match
forward_delivery_status_id
reverse_delivery_status_id
cleanup
status
```

A source-tree digest is optional if the pinned revision is exact, the tracked tree is asserted clean before build, and the actual built binary digest is recorded.

Development smoke does not need a hierarchy of candidate manifests, build manifests, placement receipts, aggregate certificates, reviewer records, and cross-job reconstruction records.

Keep raw logs, RouterInfos, identities, keys, and state disposable. Delete them before evidence upload.

## Work package 8 — authority correction: move router development forward

Update ADR 0023/status documentation to make the continuation boundary explicit.

### Work that may proceed regardless of Plan 099 wire outcome

The following is authorized after Plan 099 implementation even when the final development interop state is `protocol-defect-localized` or `environment-or-harness-blocked`:

1. production `i2pr-daemon run` composition around the existing Tokio supervisor;
2. persistent router identity load and startup/shutdown ownership;
3. local RouterInfo construction/signing/publication service architecture;
4. local NetDB validated-record store;
5. RouterInfo expiry/replacement/quota policy;
6. persistence/restart revalidation;
7. bounded DatabaseLookup/SearchReply/Store state machines under deterministic testkit execution;
8. offline/local SU3 reseed parsing, signature/trust validation, archive bounds, and RouterInfo import;
9. observability/resource ownership for those services.

These tasks do not require a working public NTCP2 transport.

### Work that still requires passing bidirectional independent NTCP2 interop

Do not do these until Plan 099 records `passed`:

1. enable NTCP2 in normal daemon operation, even behind a normal configuration flag;
2. advertise NTCP2 capability in production RouterInfo;
3. depend on NTCP2 for real NetDB peer exchange;
4. run public-network bootstrap through i2pr;
5. claim Milestone 3 transport interoperability passed;
6. start Level 2 repeated development validation.

### Level 2 timing correction

Plan 079's 3/3 repeated-direction and negative-control campaign is moved to a later integration checkpoint:

```text
before normal-daemon NTCP2 activation / public-network dependence
```

It is no longer a prerequisite for local/offline Milestone 4 implementation.

Do not delete Plan 079; amend its active status/dependency language.

## Work package 9 — production-router handoff

After Plan 099 implementation, the next **substantial** planning/execution artifact must address actual router construction.

Recommended next scope:

```text
production daemon composition
    -> router identity load
    -> supervisor/service graph
    -> local RouterInfo publication owner
    -> validated NetDB storage foundation
    -> persistence/restart revalidation
    -> deterministic local NetDB state-machine tests
    -> offline SU3 ingestion
```

Do not create Plan 100 as another NTCP2 CI/evidence plan.

If Plan 099 passes bidirectional interop, the next router plan may also include an explicitly experimental, default-disabled NTCP2 service composition seam, but capability advertisement remains false until the later validation gate.

## Detailed acceptance criteria

Plan 099 is complete only when every applicable criterion below is satisfied.

### Research/authority

1. The plan/status documentation states the narrow engineering value of independent NTCP2 interop: real second-implementation handshake + authenticated data + one correlated I2NP message in both directions.
2. Documentation clearly separates development smoke from Level 3 release qualification.
3. Rootless namespaces and Multipass are explicitly removed as prerequisites for development interoperability.
4. The active authority permits local/offline Milestone 4 work regardless of a localized NTCP2 blocker.
5. Plan 079 is deferred to the pre-normal-activation/public-network integration checkpoint rather than gating offline/local router development.

### Residual Plan 098 corrections

6. Source-commit equality is enforced correctly; no chained-inequality bug remains.
7. Reference-revision equality is enforced correctly.
8. Instrumented and control records both bind the exact i2pr binary digest.
9. Any retained i2pr manifest digest is checked for both roles.
10. There is exactly one source-tree identity algorithm if such a digest remains.
11. `build-driver.sh` Python and shell/fallback paths cannot produce different canonical source-tree digests.
12. Attempted-live preflight requires explicit `--i2pr-binary`.
13. No attempted-live runner reconstructs `target/debug/i2pr-interop` or `target/release/i2pr-interop`.

### CI simplification

14. The development interoperability workflow has one live/build job on `ubuntu-24.04`.
15. The workflow remains manual `workflow_dispatch`.
16. Binary artifacts are not uploaded/downloaded between jobs.
17. i2pr and i2pd binaries are built and executed in the same workspace/job.
18. i2pd revision is exact and worktree cleanliness is asserted.
19. i2pd build parallelism is bounded.
20. No retry action exists.
21. Live commands use literal loopback and network ID 99 only.
22. No sudo/namespaces/Multipass/Docker/public-network operation occurs during live execution.
23. Only sanitized final development evidence is retained.

### Interoperability matrix

24. Forward instrumented uses real i2pr and real pinned i2pd processes.
25. Forward pass requires exact Router Hash and DeliveryStatus correlation.
26. Forward control uses the uninstrumented reference binary and the same i2pr source/binary.
27. Reverse instrumented uses real i2pd initiator and real i2pr responder.
28. Reverse pass requires exact Router Hash and DeliveryStatus correlation.
29. Reverse control uses the uninstrumented reference binary.
30. Every attempt uses fresh mutable state, run ID, ports, and nonzero message ID.
31. No observer-only event may substitute for actual protocol success.
32. Cleanup is required before final status.
33. Passing status requires all four M1–M4 attempts to pass from the same source commit/reference revision.

### Failure/stop behavior

34. A pre-TCP failure may receive at most one narrow harness correction and one rerun before being recorded as blocked.
35. A post-TCP protocol failure is reproduced once unchanged before code modification.
36. Only a directly owned i2pr protocol defect may authorize Rust NTCP2 behavior changes.
37. After one narrow protocol correction, another nonpassing result is recorded as `protocol-defect-localized` rather than spawning another evidence architecture.
38. A localized NTCP2 defect keeps NTCP2 disabled but does not block daemon/NetDB local implementation.

### Python/artifact reduction

39. WP1 baseline tracked Rust/Python/harness LOC is recorded.
40. Total tracked Python LOC does not increase.
41. Active NTCP2 harness + interop Python LOC drops by >=40%, **or** all Plan 090–098 plan-number-specific Python execution/checking code is removed and every remaining Python file is justified as reusable functional infrastructure.
42. No new `test_plan099.py` exists solely for plan-text/workflow-string assertions.
43. No new plan-number-specific Python runner exists.
44. Default/manual development interop does not execute the full historical plan-specific Python matrix.
45. Superseded plan-specific Python tests are deleted after unique functional assertions are migrated.
46. `scripts/check-plan095-workflow.sh` is deleted or reduced/folded to durable protocol invariants; it may not remain a plan-history string checker.
47. `scripts/check-ntcp2-interoperability.sh` no longer requires obsolete status/document tokens merely to pass protocol validation.
48. Git history is treated as the archive for removed plan-specific test scaffolding; duplicate in-tree historical machinery is not retained solely for audit nostalgia.

### Router handoff

49. `i2pr-daemon run` remains non-networked until a later production-composition plan explicitly changes it.
50. The next substantial roadmap is production daemon composition + RouterInfo/NetDB foundation.
51. No new generalized NTCP2 evidence plan is registered after Plan 099 unless a real post-TCP protocol result poses one precise unresolved compatibility question.
52. NTCP2 remains experimental/non-advertised at Plan 099 closure.

## Validation baseline

Use the smallest meaningful validation set after the cleanup.

Required:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Also run the retained focused functional NTCP2 development tests, for example:

```bash
python3 -m unittest tests.integration.ntcp2.harness.test_ntcp2_development_interop
python3 -m unittest tests.integration.ntcp2.harness.test_i2pd_direct_driver
```

Use the exact retained filenames after consolidation; do not create aliases simply to match this example.

Run `scripts/check-ntcp2-interoperability.sh` only after it has been trimmed to durable active invariants.

Do not make the full historical Python plan matrix, rootless checker, Multipass checker, release certificate validator, Clippy-all-features, or rustdoc-all-workspace mandatory for this development-exit pass unless the implementation actually changes those surfaces.

## Small-model execution sequence

Execute in this exact order and commit only after the static/local work is coherent.

### Phase A — inspect and measure

1. Confirm HEAD and Plan 099 baseline.
2. Record Rust/Python/harness LOC and plan-test counts.
3. Identify unique functional assertions inside Plan 090–098 tests before deleting anything.
4. Do not edit protocol Rust code in this phase.

### Phase B — fix D1–D4

5. Fix equality semantics.
6. Unify or delete redundant source-tree digest logic.
7. Bind both roles to exact i2pr provenance.
8. Require explicit i2pr binary for live preflight.
9. Add/migrate focused functional regressions.
10. Run focused tests.

### Phase C — simplify the workflow

11. Convert the existing workflow to one job.
12. Build both implementations in place.
13. Remove binary artifact handoff.
14. Remove development-only manifest/provenance complexity that is no longer necessary.
15. Keep final binary/source digests in the compact result.
16. Run static YAML/shell sanity checks locally without emulating GitHub Actions in another framework.

### Phase D — prune Python/checker history

17. Migrate unique protocol-functional assertions to shared functional tests.
18. Delete superseded Plan 090–098 plan-specific Python tests/runners/checks where no longer needed.
19. Trim the general static checker.
20. Re-run LOC counts and verify the reduction rule.
21. Run the validation baseline.

### Phase E — commit pre-live correction

22. Commit the code/workflow/test/docs simplification before any authoritative run.
23. Ensure the working tree is clean.
24. Record the exact commit SHA as the development-interop candidate.

### Phase F — one authoritative simplified CI run

25. Manually dispatch the existing workflow exactly once against that clean commit.
26. Do not retry a failure automatically.
27. Apply the bounded failure policy above.
28. If one narrow correction is required, commit it and run exactly one replacement authoritative dispatch.

### Phase G — close and move on

29. Record `passed`, `protocol-defect-localized`, or `environment-or-harness-blocked`.
30. Reconcile Plan 087/088/079 status in one place; avoid another status-document cascade.
31. Keep NTCP2 disabled/non-advertised unless later authority changes it.
32. Begin the production-router/NetDB roadmap next.

## Expected repository state after Plan 099

Best case:

```text
independent_ntcp2_forward       = passed
independent_ntcp2_reverse       = passed
observer_control_neutrality     = passed
development_interop             = passed
level2_repetition               = deferred-to-pre-activation-checkpoint
level3_release_qualification    = environment-blocked/pending
ntcp2_normal_daemon             = disabled
ntcp2_advertised                = false
production_daemon_composition   = next
netdb_foundation                = next
```

Acceptable localized-defect case:

```text
independent_ntcp2               = protocol-defect-localized
exact_wire_stage                = recorded
one_bounded_correction          = exhausted-or-deferred
ntcp2_normal_daemon             = disabled
ntcp2_advertised                = false
production_daemon_composition   = authorized-next
local_netdb_foundation          = authorized-next
offline_reseed_parser           = authorized-next
external_netdb_over_ntcp2       = blocked-on-interop
```

Environment-blocked case after the simplified lane:

```text
independent_ntcp2               = environment-or-harness-blocked
same_job_host_loopback_attempt  = recorded
no_new_lane_architecture        = true
production_daemon_composition   = authorized-next
local_netdb_foundation          = authorized-next
ntcp2_activation                = blocked
```

## Closure principle

Plan 099 is successful only if it changes the project's direction of travel.

A technically perfect interoperability harness with an unimplemented router is not the goal. The goal is a Rust I2P router whose transport behavior is independently checked at the points where independent checking has high value.

After Plan 099, interoperability infrastructure is a **supporting test tool**, not the main product and not the dominant source of code growth.
