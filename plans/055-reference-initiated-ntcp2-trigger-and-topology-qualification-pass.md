# Plan 055: Reference-initiated NTCP2 trigger and topology qualification pass

## Status and dependencies

- Plan type: reference-driver qualification and bounded test-topology pass.
- Starting branch: `main` after Plans 053 and 054.
- Depends on:
  - Plan 053's verified run identity, observation-v2, and bundle pipeline;
  - Plan 054's stable Java per-direction state model and source-locked receiver observations.
- This plan owns only the two reference-initiated directions:
  - `java-to-i2pr-ipv4`;
  - `i2pd-to-i2pr-ipv4`.
- This plan does not own the final two complete repeated certificate runs; that is Plan 056.
- Milestone 3 remains open until Plan 056.
- NTCP2 remains experimental and non-advertised.

## Objective

Provide a source-locked, deterministic, offline method for each pinned reference router to initiate exactly one NTCP2 transport connection to the imported i2pr RouterInfo, then prove the full Plan 052 observation predicate:

- both sides observe NTCP2 authentication;
- the reference sender emits the bounded data frame;
- i2pr authenticates/decrypts the frame;
- i2pr decodes the bounded DeliveryStatus I2NP message;
- trigger provenance and target correlation are exact;
- cleanup is clean.

The preferred solution is a test-only direct transport helper linked against the unmodified pinned reference implementation. A minimal sealed support topology is permitted only after a source-inspection decision proves that no usable direct helper exists for that reference.

## Pinned references

- Java I2P 2.12.0, revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`.
- i2pd 2.60.0, revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.

No version drift is allowed in this plan.

## Governing constraints

1. The helper may call existing transport APIs but may not patch cryptography, handshake state, frame encoding, or connection acceptance.
2. The helper must target one imported RouterInfo and one synthetic endpoint.
3. The helper must not use the public I2P network, reseed servers, or Internet NetDB state.
4. The authoritative execution phase remains offline.
5. Streaming/SAM/tunnel infrastructure must not be introduced when a direct transport seam exists.
6. A support topology requires an ADR and evidence that it is the minimum source-required prerequisite.
7. Support-router traffic must never count as the target i2pr observation.
8. The target RouterInfo hash, NTCP2 static key, endpoint, and run correlation must all match the current run.
9. A trigger request is not proof of a connection. Trigger, sender observation, receiver observation, and direction result remain separate records.
10. Unknown or ambiguous helper outcomes are typed blockers, not success.
11. Helpers live under test/integration tooling and never become production dependencies.
12. Reference source modifications are forbidden for final qualification unless they are limited to a separately classified instrumented diagnostic build that cannot satisfy this plan.

## Decision process per reference

For each reference follow this order:

```text
1. inspect exact pinned source
2. identify direct existing transport seam
3. implement test-only helper against unmodified pinned artifact/libraries
4. run controls
5. accept direct helper if controls pass
6. otherwise document why direct helper is impossible
7. write/approve minimal-support-topology ADR
8. qualify topology-assisted trigger with controls
```

Do not jump directly to a support network because the existing SAM trigger is convenient.

## Workstream A: common trigger contract

### A1. Machine-readable trigger schema

Create a locked schema module, for example:

```text
tests/integration/ntcp2/harness/trigger_record.py
```

Required record fields:

```text
schema
schema_version
run_id
scenario_id
reference
reference_version
reference_revision
helper_kind
helper_binary_sha256
helper_source_sha256
target_router_hash
target_router_info_sha256
target_ntcp2_static_key_sha256
target_address
target_port
correlation_nonce
attempted
attempt_count
outcome
reason_code
transport_request_observed
connection_callback_observed
started_monotonic_ms
completed_monotonic_ms
sanitized_detail
trigger_sha256
```

The target static-key field is a digest of the public NTCP2 key, never private key material.

### A2. Bounded outcomes

At minimum:

```text
not-required-i2pr-initiator
requested
connected
authenticated
rejected-target-router-info
rejected-target-endpoint
direct-trigger-not-source-locked
direct-trigger-api-unavailable
direct-trigger-callback-timeout
direct-trigger-helper-failed
support-topology-not-approved
support-topology-not-ready
cleanup-failed
```

A final direction pass must not rely solely on `outcome = connected`; it must use the full observation predicate.

### A3. Helper build provenance

For each helper record:

- exact helper source digest;
- exact helper binary digest;
- compiler/toolchain identity;
- exact pinned headers/jars/libraries digest;
- reference revision;
- source-inspection record digest.

The helper binary must be rebuilt from the exact source commit used by the evidence run or transferred as a separately pinned artifact with a verified manifest.

### A4. One-shot behavior

The helper must invoke the selected transport seam exactly once. It must not retry silently.

If retry qualification is needed, the harness runs a new helper process with a new attempt record.

## Workstream B: i2pd direct helper

### B1. Source-inspect the exact candidate path

Verify against the pinned revision, not memory or current upstream:

- `libi2pd/Transports.cpp` and matching header;
- exact `Transports::ConnectToPeer` signature and ownership;
- `RouterInfo` load/import path;
- NetDB insertion or lookup requirements;
- callback semantics;
- transport selection logic and NTCP2 eligibility;
- thread/event-loop requirements.

Update `reference-trigger-contracts.md` with exact source lines/symbols and disposition.

### B2. Preferred helper design

Recommended location:

```text
tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/
  CMakeLists.txt or Makefile
  main.cpp
  README.md
```

The helper should:

1. parse a strict config JSON supplied by the harness;
2. load the imported i2pr RouterInfo from a run-owned file;
3. validate its hash and expected synthetic NTCP2 endpoint;
4. insert/register it using the existing pinned API;
5. invoke `ConnectToPeer` once;
6. wait for the existing callback or bounded transport state;
7. emit one structured trigger JSON;
8. exit.

It must not create a streaming destination or tunnel pool.

### B3. Running-process versus embedded-helper choice

Choose one and document it:

- **Running-router control helper:** attach through an existing supported internal control seam to the already running i2pd process.
- **Embedded test process:** initialize the minimal i2pd transport context in the helper process using pinned libraries.

Prefer the running-router model when it avoids duplicating global singleton initialization. Prefer embedded only when source inspection proves its lifecycle is deterministic and isolated.

Do not use unsafe process memory injection or debugger commands as the final trigger.

### B4. i2pd control experiments

Run all of these:

1. Correct RouterInfo and endpoint: one outbound connection attempt reaches i2pr.
2. Correct RouterInfo, wrong expected hash: helper rejects before transport call.
3. Correct hash, wrong synthetic endpoint: helper rejects or connection fails; direction cannot pass.
4. No helper invocation: i2pd does not spontaneously create the target connection in the bounded window.
5. Duplicate helper invocation: second invocation is separately recorded and cannot be merged into the first attempt.
6. Unknown RouterInfo: helper rejects.
7. Reference unavailable: helper returns typed process/control failure.
8. Successful trigger plus malformed i2pr responder: trigger may succeed, but direction remains rejected by observation.

### B5. i2pd acceptance

The i2pd direct helper is qualified only when the correct-target positive control reaches the full direction predicate and every negative control fails in the intended stage.

## Workstream C: Java direct-helper investigation

### C1. Exact source inspection

Inspect the pinned revision around:

- `NTCP2Transport` outbound establishment methods;
- `CommSystemFacade`/transport manager routing;
- RouterInfo/NetDB lookup requirements;
- `OutNetMessage` creation and target identity;
- transport bidding and direct connection establishment;
- callback/status lifecycle;
- `RouterContext` initialization requirements.

Do not assume the method names or signatures from current upstream.

### C2. Direct helper viability criteria

A Java helper is viable only if it can:

- initialize or attach to the pinned router context deterministically;
- import the exact i2pr RouterInfo without public NetDB access;
- request one transport-level send/connect to that RouterInfo;
- avoid requiring a client destination or tunnel pool;
- expose a bounded completion callback/status;
- operate with the stable Java state model from Plan 054.

### C3. Preferred helper design

Recommended location:

```text
tests/integration/ntcp2/reference-drivers/java_i2p_direct_connect/
  build.xml or build.sh
  src/.../DirectConnect.java
  README.md
```

Possible source-locked designs, in preference order:

1. A supported router-side test/application entrypoint loaded into the running pinned router context.
2. A test-only Java process using pinned router jars and a minimal `RouterContext` with imported RouterInfo.
3. An existing transport test utility in the pinned source tree adapted only through configuration and strict wrapper code.

The helper may construct a bounded DeliveryStatus `OutNetMessage` if that is the normal transport input, but it must not bypass NTCP2 framing or call i2pr directly via raw TCP.

### C4. Java controls

Use the same eight control categories as i2pd. Additionally verify:

- helper does not trigger reseed;
- helper does not start SAM or a client destination unless the selected source path explicitly requires the support-topology fallback;
- imported RouterInfo is the only target inserted for the run;
- fresh cloned Java state is used;
- helper completion is not caused by an unrelated transport connection.

### C5. Direct-helper decision record

After source inspection, record one of:

```text
java-direct-helper-selected
java-direct-helper-rejected-api-requires-netdb-only
java-direct-helper-rejected-requires-tunnel-pool
java-direct-helper-rejected-global-context-not-isolatable
java-direct-helper-rejected-no-bounded-callback
```

A rejection must cite exact source symbols and call graph. “Could not get it working” is insufficient.

## Workstream D: minimal sealed support topology fallback

### D1. ADR gate

A support topology may be implemented only after an ADR, likely `docs/adr/0020-...md`, states:

- which reference lacks a direct seam;
- exact source evidence for the missing seam;
- why the public control API requires NetDB/tunnel readiness;
- minimum support-router roles;
- isolation and no-egress design;
- how target connection correlation remains unambiguous;
- why this does not test more than required.

Without the ADR, return:

```text
support-topology-not-approved
```

### D2. Determine minimum topology empirically and from source

Start from the smallest candidate. Do not hard-code a large miniature network.

Candidate progression:

1. one pinned support router acting as the required NetDB/floodfill role;
2. add one second support router only if tunnel construction source requirements prove one is insufficient;
3. use zero-hop or minimum-hop test tunnel settings only when supported by the pinned reference and documented as qualification-only configuration;
4. stop adding routers once the public trigger reaches readiness reliably.

Record why each added router is required.

### D3. Topology constraints

- all routers run inside the same sealed rootless network namespace or an equivalently attested isolated topology;
- no default external route;
- no reseed URLs;
- no public floodfill access;
- static RouterInfo exchange only;
- fixed synthetic addresses;
- pinned reference implementations only;
- support routers cannot share identity/state with target reference or i2pr;
- support-router logs and observations are diagnostic only;
- only the target reference-to-i2pr connection can satisfy the direction.

### D4. Readiness contract

Support topology readiness must be explicit, not sleep-based. Depending on the source requirement, prove:

- required RouterInfos loaded;
- required floodfill visibility available;
- required inbound/outbound test tunnels ready;
- no external route;
- target i2pr RouterInfo imported;
- no target connection occurred before trigger.

### D5. Topology control experiments

In addition to direct-helper controls:

- support topology ready but no reference trigger: no target connection;
- trigger with i2pr RouterInfo withheld: no target connection;
- trigger with wrong target endpoint: no pass;
- support router receives the bounded message instead of i2pr: reject correlation;
- remove one support role: readiness must fail with the predicted typed blocker;
- parent network digest remains unchanged.

## Workstream E: integrate triggers into Plan 052 records

### E1. Trigger selection is explicit

The scenario configuration or locked reference-trigger catalog must name one of:

```text
i2pd-direct-helper
java-direct-helper
java-minimal-support-topology
```

No automatic fallback from direct helper to support topology is allowed in the same run.

### E2. Bind trigger and observations

The direction record must bind:

- trigger record digest;
- helper binary digest;
- target RouterInfo digest;
- target router hash;
- correlation nonce;
- sender observation digest;
- receiver observation digest;
- run identity digest.

All values must agree before a direction may pass.

### E3. Preserve i2pr responder stages

When i2pr rejects the reference-initiated handshake, retain the new bounded responder reason from the Rust launcher. The trigger can be successful while the direction is rejected, for example:

```text
trigger outcome: connected
direction reason: responder-session-confirmed-part2-failed
```

Do not relabel this as a trigger failure.

## Required tests

### Trigger schema tests

- malformed target hash rejected;
- wrong endpoint type rejected;
- zero helper digest rejected for attempted trigger;
- trigger digest round-trip;
- unknown helper kind rejected;
- attempt count other than the declared one-shot contract rejected;
- run identity mismatch rejected.

### i2pd helper tests

- source revision mismatch fails build/launch;
- correct imported RouterInfo reaches transport call;
- wrong hash rejected before call;
- wrong endpoint cannot pass;
- callback timeout typed;
- no spontaneous target connection;
- helper exits and leaves no residual process.

### Java helper tests

Equivalent controls plus:

- no reseed attempt;
- no mutable template sharing;
- no SAM/client destination when direct mode selected;
- helper uses pinned jars only;
- target OutNetMessage/transport request is correlated.

### Support topology tests, if selected

- ADR presence and digest required;
- exact role inventory required;
- external route absent;
- static RouterInfo inventory complete;
- readiness is event-based;
- support traffic cannot satisfy target predicate;
- removing a required role yields typed readiness blocker;
- cleanup removes every support process.

### End-to-end qualification tests

For each reference-initiated direction:

- positive control reaches full Plan 052 predicate;
- no-trigger control fails;
- wrong-RouterInfo control fails;
- wrong-address control fails;
- malformed responder/data control fails at expected stage;
- repeated independent run produces the same typed positive outcome;
- bundle verifies before and after export.

## Suggested commit sequence for a smaller model

1. `interop: add locked reference trigger record schema`
2. `interop: source-lock i2pd direct transport seam`
3. `interop: implement i2pd direct-connect test helper`
4. `tests: qualify i2pd direct trigger controls`
5. `interop: source-inspect Java direct transport seam`
6. Either:
   - `interop: implement Java direct-connect test helper`, or
   - `docs: add ADR for minimal Java support topology`
7. If needed: `interop: implement minimal sealed support topology`
8. `interop: bind reference triggers to Plan 052 observations`
9. `tests: close reference-initiated direction qualification matrix`
10. `docs: record Plan 055 qualification status`

Do not implement both a Java direct helper and a support topology before deciding which is required.

## Smaller-model execution guidance

### Trace call graphs before coding

Write the exact call graph in `reference-trigger-contracts.md` first. Include source path and symbol for every hop that determines whether NetDB, tunnels, or a global context is required.

### Keep helper behavior minimal

Incorrect:

```text
start SAM
create transient destination
wait for tunnels
open streaming connection
```

when a direct transport seam exists.

Correct:

```text
load target RouterInfo
validate hash/address
invoke existing transport connect/send path once
record callback
```

### Do not confuse trigger success with protocol success

A helper that requests a connection has only proven trigger dispatch. The direction passes only after both observation records and i2pr terminal status satisfy the locked predicate.

### Avoid uncontrolled retries

If the reference API retries internally, source-inspect and record that behavior. The helper itself must not add retries. Count actual connection attempts from reference observations.

### Stop conditions

Stop and document a typed blocker when:

- the candidate symbol does not exist at the pinned revision;
- the only available direct path bypasses the real NTCP2 transport;
- a helper requires patching cryptographic/transport behavior;
- target correlation cannot exclude support-router traffic;
- a support topology would require public reseed/network access;
- the minimum support-router inventory cannot be justified;
- Java global context cannot be isolated between attempts;
- cleanup leaves residual reference/support processes.

## Explicit acceptance criteria

Plan 055 is complete only when all of the following are true:

1. A locked machine-readable trigger schema and validator exist.
2. Every trigger helper is bound to exact source and binary digests.
3. Exact source inspection is complete for i2pd and Java.
4. i2pd uses a qualified direct helper, or a source-cited typed rejection explains why not.
5. Java uses a qualified direct helper or an ADR-approved minimal support topology.
6. No unapproved automatic fallback exists.
7. All required positive and negative trigger controls pass.
8. Trigger success cannot independently mark a direction passed.
9. Target RouterInfo hash, public NTCP2 key digest, endpoint, nonce, and run identity all correlate.
10. `i2pd-to-i2pr-ipv4` reaches the full Plan 052 observation predicate in two independent qualification attempts.
11. `java-to-i2pr-ipv4` reaches the full Plan 052 observation predicate in two independent qualification attempts.
12. New bounded i2pr responder-stage reasons remain visible in rejected records.
13. No public network, reseed, or external NetDB dependency is used.
14. Every helper/support process is cleaned up.
15. The parent network state remains unchanged.
16. Verified Plan 052 diagnostic bundles retain all four direction records.
17. Full Python, Rust, static-boundary, and documentation checks pass.
18. `plans/055-status.md` records selected seams/topology, exact revisions, controls, and outcomes.
19. Milestone 3 and advertised NTCP2 support remain unclaimed until Plan 056.

## Required handoff artifacts

- Machine-readable trigger schema and tests.
- Source-inspection updates with exact call graphs.
- Qualified helper source/build manifests.
- ADR and topology manifest if fallback is required.
- Positive and negative control records.
- Updated Plan 052 diagnostic bundle(s).
- `plans/055-status.md`.
