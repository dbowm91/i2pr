# Plan 059: reference-side implementation and live qualification closure pass

## Status and dependencies

- Plan type: corrective implementation and live qualification closure pass.
- Starts only after Plan 058 closes.
- Requires ADR 0021 to be explicitly Accepted. If ADR 0021 is Rejected, this plan must close with a typed blocker and Plan 060 must not start under the current four-direction contract.
- Owns all missing reference-side implementation and qualification that Plans 054-057 deferred.
- Does not freeze the final Milestone 3 candidate and does not produce the final two-run certificate. Plan 060 owns those actions.
- Milestone 3 remains open. NTCP2 remains experimental and non-advertised.

## Objective

Complete and qualify every reference-side mechanism needed before a clean external candidate can be frozen:

1. implement a source-locked i2pd direct NTCP2 trigger helper;
2. implement the ADR-approved minimal sealed Java support topology;
3. replace provisional receiver-observation strings with source-verified, runtime-demonstrated observation mechanisms;
4. execute the Java startup matrix and select one stable immutable state model;
5. execute positive and negative trigger/observation controls for both pinned references;
6. produce at least one complete live four-direction diagnostic bundle that exercises the canonical Plan 052/053 pipeline without claiming a Milestone 3 certificate;
7. leave the repository implementation-complete and ready for a new Plan 060 candidate freeze.

## Why this plan is separate from Plan 060

The current repository has schemas, validators, and source-inspection notes but lacks the actual reference-side execution machinery. Freezing a candidate before implementing and qualifying that machinery repeats the Plan 056 ordering defect.

Plan 059 may change code, scripts, test helpers, source-lock catalogs, ADR implementation notes, and harness behavior. Plan 060 may not.

## Fixed references

Unless a separate ADR changes them, use exactly:

- Java I2P 2.12.0 at `2800040deee9bb376567b671ef2e9c34cf3e30b6`;
- i2pd 2.60.0 at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.

Any drift in reference revision, artifact digest, installed-tree digest, or helper build inputs is a typed blocker.

## Scope

### In scope

- `tests/integration/ntcp2/reference-drivers/`
- `tests/integration/ntcp2/harness/trigger_record.py`
- `tests/integration/ntcp2/harness/reference_trigger.py`
- `tests/integration/ntcp2/harness/i2pd.py`
- `tests/integration/ntcp2/harness/java_i2p.py`
- `tests/integration/ntcp2/harness/observation_catalog.py`
- `tests/integration/ntcp2/harness/observation_helpers.py`
- `tests/integration/ntcp2/reference-observation-catalog.toml`
- `tests/integration/ntcp2/reference-trigger-contracts.md`
- Java template/matrix tooling
- rootless topology composition needed by the Java support topology
- Plan 059 tests, static checks, status and qualification receipts

### Out of scope

- changing NTCP2 wire semantics merely to make a reference accept i2pr;
- patching reference cryptography, handshake state, frame encoding, or acceptance behavior;
- public-network execution;
- changing the four primary direction IDs;
- final two-run certificate production;
- support advertisement.

## Non-negotiable reference-integrity rule

Reference helper or observer code may call existing reference APIs and inspect existing successful states. It may not alter the protocol behavior being tested.

Allowed:

- a separately built test executable linked to pinned reference libraries;
- source-locked callback registration;
- existing debug/event counters;
- exact existing log markers;
- read-only state inspection;
- a minimal test-only observer inserted only if separately approved by an ADR and proven not to change transport behavior.

Forbidden:

- bypassing authentication;
- forcing session state to authenticated;
- injecting decoded I2NP success directly;
- changing keys, nonces, AEAD, framing, RouterInfo validation, timeouts, or connection acceptance;
- treating sender counters as receiver proof;
- adding invented log strings to the catalog without a runtime source.

## Workstream A: execution-host qualification fixture

Before implementing helpers, provision one authorized Ubuntu 24.04 amd64 qualification environment using either Plan 058 Lane A or Lane B.

Required environment properties:

- rootless probe returns `rootless_sandbox_available` in the actual execution kernel;
- at least one fresh owned execution environment;
- pinned Java and i2pd references can build or install from verified cache;
- no public egress during qualification directions;
- source and reference trees are available for source inspection;
- enough memory and disk to run one reference plus i2pr, and for Java topology controls;
- diagnostics mode is `sanitized` unless a local non-exported `raw-local` investigation is explicitly needed.

Create a sanitized qualification environment receipt with:

```text
schema
lane_kind = direct-host | guest
host_baseline_result
guest_rootless_result (typed absence for direct-host lane)
reference_lock_sha256
environment_manifest_sha256
java_installed_tree_sha256
i2pd_installed_tree_sha256
```

No private RouterInfo or key material may enter the receipt.

## Workstream B: implement the i2pd direct helper

### B1. Source-lock the call graph

Confirm against the pinned i2pd revision:

- the selected public entry path;
- RouterInfo insertion path;
- transport subsystem initialization order;
- one-shot callback or terminal result path;
- cleanup/shutdown path.

Update `tests/integration/ntcp2/reference-trigger-contracts.md` with exact file paths, symbols, and line ranges from the pinned revision.

Do not rely on prose such as “ConnectToPeer exists.” Record the actual callable public path selected by the helper.

### B2. Helper location and build contract

Recommended layout:

```text
tests/integration/ntcp2/reference-drivers/i2pd/
  CMakeLists.txt or build.sh
  i2pd_direct_connect.cpp
  README.md
  source-lock.json
```

The helper must be built from pinned installed headers/libraries or the pinned source tree. Record:

- compiler identity;
- compile flags;
- linked library digests;
- helper source digest;
- helper binary digest;
- pinned i2pd revision;
- source inspection record digest.

### B3. Required helper interface

Example command shape:

```bash
i2pd-direct-connect \
  --data-dir <fresh-run-dir> \
  --router-info <i2pr-router-info-path> \
  --expected-router-hash <base64-or-hex-hash> \
  --expected-host 192.0.2.1 \
  --expected-port 45680 \
  --result <trigger-record.json>
```

The helper must:

1. validate the target RouterInfo before starting transports;
2. reject a target hash mismatch;
3. reject a target endpoint mismatch;
4. add only the declared RouterInfo to the disposable reference NetDB;
5. start the minimum required i2pd transport context;
6. request exactly one outbound peer connection;
7. wait for a bounded callback/terminal result;
8. emit one `i2pr-reference-trigger-v3` record;
9. shut down all helper-owned reference state;
10. return nonzero for rejected/blocked outcomes.

### B4. i2pd helper controls

Run all controls inside the sealed topology:

| Control | Expected result |
| --- | --- |
| correct RouterInfo and endpoint, i2pr listening | `authenticated` or next typed i2pr responder failure after TCP connect |
| wrong RouterInfo hash | `rejected-target-router-info` before TCP connect |
| wrong endpoint | `rejected-target-endpoint` before or at connect, never authenticated |
| no i2pr listener | bounded connect failure/timeout |
| no trigger invocation | zero connection attempt |
| invoke helper twice | second invocation rejected by one-shot run contract |
| stale helper binary digest | provenance rejection |
| changed pinned i2pd tree | reference-digest mismatch blocker |

The positive control need not make the full direction pass while protocol defects remain, but it must prove the helper actually causes the pinned i2pd transport to dial the imported i2pr RouterInfo.

## Workstream C: implement the Java minimal support topology

### C1. ADR gate

Do not start until ADR 0021 is `Accepted`.

If the ADR is Rejected, write `plans/059-status.md` with:

```text
blocked_java_support_topology_rejected
```

and stop. Do not invent an unsupported direct helper.

### C2. Select the support-router implementation

The topology permits exactly:

- one pinned Java reference under test;
- one pinned support router supplying the minimum NetDB visibility;
- i2pr responder.

Select the support router only after documenting why it supplies the required NetDB visibility with the least additional machinery. The support router may be Java I2P or i2pd if the pinned implementation can satisfy the role deterministically.

Record the decision in ADR 0021 implementation notes:

- implementation and revision;
- exact role;
- static RouterInfo exchange path;
- why no smaller topology works;
- fixed synthetic addresses and ports;
- support-router identity/state generation contract;
- cleanup contract.

### C3. Recommended topology interface

Add a dedicated topology/helper object rather than embedding shell commands in `mixed_runner.py`.

Suggested module:

```text
tests/integration/ntcp2/harness/java_support_topology.py
```

Suggested API:

```python
class JavaSupportTopology:
    def prepare(self, *, run_context, java_state, i2pr_router_info) -> SupportReceipt: ...
    def start(self) -> None: ...
    def trigger_java_dial(self) -> TriggerRecord: ...
    def stop(self) -> CleanupRecord: ...
```

The topology must expose bounded records only. It must not expose RouterInfo contents, private keys, or full logs.

### C4. Required topology behavior

1. Create fresh per-direction support state.
2. Start the support router without public reseed or external routes.
3. Import only the required static RouterInfos.
4. Start the Java reference from a seeded clone.
5. Wait for explicit support readiness.
6. Ensure the support router does not dial i2pr.
7. Trigger exactly one Java-originated path that results in Java opening NTCP2 to i2pr.
8. Record the Java trigger as `java-minimal-support-topology`.
9. Prove the observed inbound connection at i2pr came from the Java reference endpoint, not the support router.
10. Shut down Java, support router, and all topology helpers.

### C5. Java topology controls

Mandatory controls:

| Control | Expected result |
| --- | --- |
| correct topology + i2pr listener | Java reference opens the intended connection |
| support router removed | Java trigger cannot complete; typed `support-topology-not-ready` |
| i2pr RouterInfo absent | no Java connection attempt to i2pr |
| wrong i2pr RouterInfo | rejected target/no authentication |
| support router tries to dial i2pr | run rejected; support traffic cannot satisfy direction |
| public route injected | isolation attestation rejects run |
| stale support identity/config digest | provenance rejection |
| duplicate trigger | one-shot attempt-count rejection |
| cleanup leaves support process | `failed_cleanup` |

### C6. Do not use streaming success as proof

The Java trigger may use the minimum existing router machinery required to cause transport dialing, but the evidence predicate remains transport-level:

- Java endpoint opens TCP to i2pr;
- both sides authenticate NTCP2;
- Java emits the bounded data frame when it is sender;
- i2pr receiver decrypts and decodes the correlated I2NP message.

A streaming session or tunnel becoming ready is not itself evidence.

## Workstream D: qualify receiver-side observations

### D1. Audit the current catalog

For each current marker in `reference-observation-catalog.toml`, prove one of:

1. exact marker exists in the pinned source and is emitted by the pinned runtime configuration;
2. exact counter/callback exists and can be exposed through a source-locked read-only observer;
3. marker is invalid and must be replaced.

Do not retain a marker solely because unit tests can scan a fixture containing that string.

### D2. Qualification receipt per marker

Add machine-readable qualification records, for example:

```text
tests/integration/ntcp2/reference-observation-qualification/
  java_i2p-2.12.0.json
  i2pd-2.60.0.json
```

Each semantic level must record:

```text
reference
revision
semantic_level
source_path
symbol
observation_kind
exact_marker_or_counter
source_excerpt_sha256
runtime_control_run_id
positive_count
negative_control_count
sanitization_rule
qualified = true|false
```

Do not commit source excerpts; commit only their digest and bounded symbol/path metadata.

### D3. Positive and negative receiver controls

For each reference receiver:

- valid authenticated frame + valid bounded DeliveryStatus I2NP → decrypt and decode observed;
- handshake only, no data frame → neither data level observed;
- malformed AEAD frame → decrypt not observed;
- valid frame carrying invalid I2NP → decrypt observed, decode not observed;
- stale pre-run log marker → ignored by log cursor;
- wrong correlation nonce/message ID → decode event not accepted for this run;
- duplicate unrelated messages → only correlated message counts.

### D4. Observer-only instrumentation decision

If the pinned reference exposes no existing observation surface sufficient to distinguish decrypt from decode:

- stop and document the gap;
- do not invent a catalog string;
- open an ADR specifically for observer-only reference instrumentation;
- require proof that instrumentation adds logging/counter observation only after existing success branches and does not alter behavior;
- include instrumented source patch digest in reference provenance;
- require both instrumented and uninstrumented control runs to show identical protocol outcomes.

Plan 059 cannot silently patch the reference.

## Workstream E: execute and close the Java startup matrix

Run the complete 16-cell matrix with three attempts per cell in the authorized environment.

Variables:

- namespace: outer/direct vs rootless child;
- state: empty vs seeded clone;
- launcher: runplain vs wrapper;
- sequence: single vs generate-live/restart.

Then execute the selected production cell for ten consecutive starts.

### Selection rule

Prefer the least stateful passing cell that satisfies all requirements. If only seeded-clone works, record that as the canonical execution model.

Required selected-cell properties:

- 10/10 starts reach readiness;
- 10/10 clean shutdowns;
- no static template mutation;
- fresh clone digest differs only in allowlisted mutable files;
- no lock or process residue;
- entropy probe remains within bounded latency;
- no `Random is shut down` failure;
- identical result under the selected external lane.

### Failure taxonomy

Keep stage-specific outcomes such as:

```text
java-process-spawn-failed
java-seed-read-failed
java-random-source-shutdown
java-router-context-init-failed
java-ntcp2-listener-not-ready
java-state-lock-invalid
java-cleanup-failed
```

Do not collapse them into `java-startup-failed`.

## Workstream F: integrate live helpers and observations into the canonical runner

The canonical path must consume actual trigger and observation records:

```text
rootless-enter.sh
  -> rootless_inner_runner.py
  -> mixed_runner.py
  -> reference helper/topology
  -> adapter collect_observation()
  -> plan052_pipeline.write_direction_artifacts()
```

Required rules:

- synthetic fallback is allowed only for blocked/diagnostic fixture runs;
- a `passed` direction requires a live trigger record when the reference is initiator;
- a `passed` direction requires live sender and receiver observation-v2 records;
- helper and catalog digests bind into run identity and direction records;
- helper outcome cannot mask i2pr terminal failure;
- cleanup failure overrides pass;
- unknown or unqualified observation marker blocks pass;
- support topology receipt is mandatory for Java reference-initiated direction.

## Workstream G: one complete live qualification bundle

Before Plan 060 candidate freeze, execute one full four-direction live diagnostic run from a clean implementation commit.

This is not the final two-run certificate. Its purpose is to prove that all four directions can enter the live machinery and produce complete records.

Acceptable outcome:

- all four directions pass; or
- one or more directions return a typed protocol failure after the intended trigger and observation mechanisms were exercised.

Unacceptable outcome:

- helper missing;
- marker unqualified;
- Java topology not approved/not implemented;
- synthetic fallback used for a live direction;
- evidence context missing;
- generic harness-operation failure;
- incomplete bundle.

The run must produce:

- complete immutable bundle;
- four trigger records;
- four observation records;
- four cleanup records;
- exact source/reference/helper/catalog provenance;
- rootless attestation;
- parent-network before/after equality;
- sanitized qualification summary.

If a real protocol defect remains, Plan 059 may fix it only when the defect is within the already scoped bounded NTCP2 implementation. After any fix, repeat the live qualification bundle from a new clean commit.

## Required tests

Add `tests/integration/ntcp2/harness/test_plan059.py` covering at least:

### i2pd helper

1. valid source-lock record;
2. helper digest binding;
3. correct target positive fixture;
4. wrong RouterInfo rejection;
5. wrong endpoint rejection;
6. no-listener timeout;
7. duplicate attempt rejection;
8. cleanup failure override.

### Java support topology

9. ADR Accepted gate;
10. ADR Proposed/Rejected prevents implementation activation;
11. exact support inventory;
12. support-router removal control;
13. support-router traffic cannot satisfy Java direction;
14. no public route;
15. topology digest binding;
16. cleanup residue rejection.

### receiver observations

17. source-qualified marker required;
18. valid decrypt+decode positive;
19. handshake-only negative;
20. malformed-frame negative;
21. invalid-I2NP split outcome;
22. stale cursor rejection;
23. correlation mismatch rejection;
24. unqualified marker rejection.

### Java startup

25. matrix has exactly 16 cells and 48 attempts;
26. selected cell requires 10/10 starts;
27. template launch forbidden;
28. template mutation rejected;
29. residual process/lock rejected.

### pipeline

30. live direction cannot use synthetic fallback;
31. reference-initiated pass requires trigger record;
32. pass requires live observation-v2;
33. helper/catalog digests cross-check;
34. one complete four-direction qualification bundle fixture.

## Validation commands

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh

cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
```

External qualification commands and receipts must be recorded in `plans/059-status.md`.

## Explicit closure criteria

Plan 059 closes only when every applicable item is true:

- [ ] Plan 058 is closed.
- [ ] ADR 0021 is Accepted. If Rejected, Plan 059 closes only with a typed blocker and Plan 060 is prohibited.
- [ ] The i2pd direct helper source, build contract, source-lock record, and binary provenance are committed.
- [ ] The i2pd helper passes all positive and negative controls against the pinned revision.
- [ ] The Java support topology implementation is committed and matches ADR 0021 exactly.
- [ ] The Java topology passes support-removal, wrong-target, no-public-egress, traffic-origin, one-shot, and cleanup controls.
- [ ] Support-router traffic cannot satisfy the Java primary direction.
- [ ] Every Java and i2pd decrypt/decode observation is source-qualified and runtime-demonstrated.
- [ ] Provisional or invented markers are removed.
- [ ] Observation negative controls prove handshake-only and malformed data cannot pass.
- [ ] The complete Java 48-attempt matrix is retained as sanitized evidence.
- [ ] One selected Java cell passes 10 consecutive start/shutdown cycles.
- [ ] The frozen Java template is immutable and execution uses fresh seeded clones.
- [ ] The canonical runner consumes live trigger and observation records.
- [ ] A live direction cannot pass through synthetic fallback.
- [ ] One complete four-direction live qualification bundle is produced and verifies.
- [ ] No direction is blocked by missing helper, unapproved topology, unqualified marker, or missing evidence context.
- [ ] Any remaining failures are typed protocol outcomes reached after actual reference initiation/observation.
- [ ] All local and external validation commands pass.
- [ ] `plans/059-status.md` records exact implementation commit, qualification environment, artifact digests, controls, and remaining protocol defects.
- [ ] No final candidate has yet been frozen.
- [ ] Milestone 3 remains open and NTCP2 remains non-advertised.

## Stop conditions

Stop without claiming implementation closure when:

- ADR 0021 is not Accepted;
- no source-safe i2pd helper path can be implemented;
- Java topology needs more than the ADR-authorized roles;
- reference receiver decrypt/decode cannot be observed without behavior-changing patches;
- Java selected cell cannot pass 10/10 starts;
- support router traffic cannot be distinguished from Java traffic;
- helper or topology cleanup is unreliable;
- external environment cannot maintain sealed execution.

Use typed blockers such as:

```text
blocked_i2pd_direct_helper_unavailable
blocked_java_support_topology_not_approved
blocked_java_support_topology_insufficient
blocked_reference_receiver_observation_unavailable
blocked_java_startup_not_reproducible
blocked_reference_helper_cleanup_failed
blocked_external_qualification_environment
```

## Smaller-model execution sequence

Execute in this order and do not skip ahead:

1. verify Plan 058 and ADR status;
2. implement/test i2pd helper only;
3. qualify i2pd observations only;
4. run Java matrix and freeze selected state model;
5. implement Java support topology only;
6. qualify Java observations only;
7. wire canonical runner;
8. run one direction at a time;
9. run one complete four-direction qualification bundle;
10. write status and stop before candidate freeze.

Suggested commits:

1. `interop: add source-locked i2pd direct trigger helper`
2. `tests: qualify i2pd trigger controls`
3. `interop: qualify i2pd receiver observations`
4. `interop: close Java startup matrix and freeze state model`
5. `interop: implement ADR 0021 Java support topology`
6. `tests: qualify Java topology controls`
7. `interop: qualify Java receiver observations`
8. `interop: wire live reference records into canonical pipeline`
9. `tests: add Plan 059 full regression matrix`
10. `docs: record Plan 059 live qualification closure`

## Required handoff artifacts

- i2pd helper source/build/source-lock files;
- Java support topology implementation and receipt schema;
- updated Accepted ADR 0021 implementation notes;
- qualified observation catalog and qualification receipts;
- Java matrix summary and selected-cell qualification receipt;
- Plan 059 test suite;
- one complete live four-direction diagnostic bundle or a durable sanitized manifest/receipt according to repository evidence policy;
- `plans/059-status.md`.
