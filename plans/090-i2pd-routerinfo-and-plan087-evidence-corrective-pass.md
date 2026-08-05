# Plan 090: i2pd RouterInfo and Plan 087 evidence corrective pass

## Status and authority

- Status: planned.
- Parent roadmap: Plan 085.
- Corrective target: the open Plan 087 forward direction.
- Reuses: Plan 064/076 pinned i2pd direct driver, Plan 083 forward runner, Plan 086 host-loopback placement, and the Plan 087 execution contract.
- Blocks: Plan 088, Plan 079, and any claim of two-way development interoperability.
- Does not reopen: Plan 086 lane qualification. Plan 086 remains `host-loopback-development-ready` unless this work demonstrates a placement failure independent of the RouterInfo defect.
- Plan type: narrow reference-driver and evidence-semantics correction followed by one clean committed-head Plan 087 rerun.

## Current baseline

The Plan 083 runner correction and the first Plan 087 attempt established the following facts:

1. The host-loopback lane can prepare both peers, start the pinned i2pd listener, observe an authentic `listener_ready`, start the real i2pr dialer while the listener remains alive, drain both event streams, and cleanly reap both processes.
2. The Plan 087 instrumented attempt did not reach TCP.
3. The i2pr dialer rejected the copied i2pd RouterInfo with `peer_router_info_invalid`.
4. The i2pd direct driver's exported signed `router.info` decodes with zero `RouterAddress` entries, so i2pr cannot find the exact expected NTCP2 endpoint.
5. A standalone pinned i2pd process can produce an NTCP2 RouterAddress under an equivalent loopback configuration, which localizes the defect to the bounded direct-driver initialization, update, or export path rather than to the pinned i2pd implementation generally.
6. The retained Plan 087 record is semantically inaccurate: it records `protocol_rejected` / `reference-events-missing` even though no `tcp_connected` event occurred and the i2pr terminal reason was a pre-protocol RouterInfo rejection.
7. The retained attempt was executed from a working tree whose runner corrections were committed afterward, so it is diagnostic evidence but not canonical Plan 087 closure evidence.
8. `plans/088-status.md` is stale and still describes Plan 086 as unclosed and Plan 087 as unexecuted.

This plan corrects those issues without broadening protocol scope or modifying production i2pr NTCP2 behavior.

## Objective

Produce a trustworthy Plan 087 result from a clean committed source tree by:

1. correcting the Plan 064/076 i2pd direct driver so its exported signed RouterInfo contains the exact configured NTCP2 endpoint;
2. proving the correction through deterministic structural tests for both instrumented and control builds;
3. correcting Plan 083's pre-TCP classification and placement ownership;
4. rebuilding and measuring all executed binaries from the corrective commit;
5. rerunning Plan 087 from fresh state;
6. recording an instrumented pass and a behavior-neutral control result before enabling Plan 088.

The plan answers two bounded questions:

```text
Does the pinned i2pd direct driver export a valid signed RouterInfo containing
exactly the NTCP2 endpoint on which its real listener is configured?

Can the current i2pr initiator authenticate to that pinned i2pd responder and
complete the required DeliveryStatus exchange from a clean committed head?
```

## Non-goals

This plan must not:

- change production i2pr NTCP2 handshake, framing, crypto, RouterInfo parsing, or transport policy;
- weaken `exact_ntcp2_address` or accept a RouterInfo without the exact expected endpoint;
- manually inject bytes into a serialized RouterInfo;
- fabricate a RouterAddress in the Python harness;
- patch pinned i2pd NTCP2 handshake, encryption, frame processing, or transport code;
- alter the passive observer event positions except where a rebuild is required to preserve the existing instrumentation contract;
- add a fallback to SAM, I2CP, HTTP, SOCKS, a normal router daemon, public NetDB bootstrap, reseed, DNS, or SSU2;
- activate Plan 089, because the demonstrated failure is not a placement or bind failure;
- execute Plan 088 in the same implementation pass;
- add CI workflows, containers, VMs, broad qualification bundles, or new evidence frameworks;
- treat a listener-ready result, a TCP connection alone, or a typed protocol failure as a Plan 087 pass.

## Invariants

The following invariants are mandatory throughout execution:

```text
reference revision                    = pinned i2pd 2.60.0 revision
network ID                            = 99
topology                              = host-loopback-development
endpoint                              = literal IPv4 127.0.0.1:<fresh-port>
development_only                      = true
release_qualified                     = false
isolation_qualified                   = false
instrumented/control transport path   = behaviorally identical
observer instrumentation              = passive and compile-time gated
peer RouterInfo                       = signed bytes produced by real i2pd
peer endpoint acceptance              = exact address and port match
source attribution                    = clean committed SHA
process ownership                     = HostLoopbackDevelopmentPlacement
cleanup                               = bounded and clean
Plan 088                              = blocked until all closure criteria pass
```

## Required deliverables

### D1. Driver source diagnosis

Document the exact pinned i2pd call path that determines the local RouterInfo address set, including:

- configuration values applied before `i2p::context.Init()`;
- the `RouterContext::CreateNewRouter()` / `NewRouterInfo()` path;
- when `AddNTCP2Address(...)` is expected to run;
- when `RouterContext::UpdateRouterInfo()` or equivalent update logic runs;
- when transports bind the configured NTCP2 endpoint;
- whether a pre-existing data-directory RouterInfo is loaded and rewritten;
- which authoritative RouterInfo buffer or file should be exported after initialization;
- the exact point at which the address is absent or removed.

Record the result in the existing source-verification documentation or a tightly scoped adjacent note. Do not produce an unreferenced speculative explanation.

### D2. Behavior-neutral i2pd direct-driver correction

Correct only the bounded driver lifecycle/configuration/export path required to make the real pinned i2pd RouterInfo carry the configured NTCP2 address.

Preferred correction order:

1. correct option ordering or option values before `context.Init()`;
2. use an existing pinned i2pd RouterContext refresh/update API at the lifecycle point expected by i2pd;
3. export the authoritative signed RouterInfo only after that normal update path completes;
4. ensure a reused data directory cannot retain a stale zero-address RouterInfo.

The implementation must not:

- construct a RouterAddress in Python;
- edit serialized RouterInfo bytes;
- sign a harness-created RouterInfo;
- modify pinned i2pd transport behavior;
- add a driver-only fake endpoint that the listener does not actually own;
- force the i2pr parser to accept missing or mismatched addresses.

The driver must fail closed with a typed terminal rejection if it cannot verify its exported RouterInfo contains the configured endpoint.

### D3. Deterministic RouterInfo structural verification

Add a focused verifier/test that consumes the exact file written by the direct driver and proves all of the following:

```text
file exists
file is non-empty and within RouterInfo bounds
RouterInfo decodes with the repository's real decoder
RouterInfo signature verifies
network ID is 99
addresses.len() > 0
exactly one acceptable NTCP2 address resolves to 127.0.0.1:<configured-port>
NTCP2 static key is present and structurally valid
NTCP2 IV is present and structurally valid
no public endpoint is present
no hostname or DNS endpoint is present
no SSU/SSU2 endpoint is introduced
```

Run this verification against:

- a fresh instrumented driver data directory;
- a fresh control driver data directory;
- a second invocation using the same owned data directory, proving the address survives load/update/export rather than only first creation.

The test must compare the exported endpoint to the actual configured listener endpoint. `addresses.len() > 0` alone is insufficient.

### D4. Instrumented/control parity guard

Prove that the instrumented and control driver builds:

- use the same pinned i2pd revision;
- use the same configuration and lifecycle path;
- produce valid signed RouterInfos containing the same endpoint contract;
- start the same real NTCP2 listener path;
- differ only by the passive observer compilation surface and expected binary/build digests.

Do not require byte-identical RouterInfos because identities and timestamps may differ. Require semantic equivalence of address family, endpoint, network ID, and transport type.

### D5. Plan 083 pre-TCP classification correction

Update the canonical forward runner so terminal classification is derived from authenticated current-run evidence and the explicit i2pr terminal status.

Required classification rules:

```text
No tcp_connected + i2pr peer_router_info_invalid
  -> terminal_result = pre_protocol_rejected
  -> reason_code = pre-protocol-router-info-validation-failed
  -> highest_stage_reached may remain listener_ready when the real listener was ready

No tcp_connected + scenario validation failure
  -> terminal_result = pre_protocol_rejected
  -> reason_code = pre-protocol-render-failed

No tcp_connected + listener unavailable
  -> terminal_result = pre_protocol_rejected
     or the existing exact pre-protocol listener reason required by the schema

reference-events-missing
  -> allowed only after authentic peer progress where the reference event stream
     should contain a required event but does not
```

A generic `protocol_rejected` result is forbidden unless at least one authentic `tcp_connected` event exists.

Add a bounded reason mapping for every i2pr terminal reason that can occur before TCP. Unknown pre-TCP terminal reasons must fail closed into an explicit pre-protocol unknown/rejected category, not `reference-events-missing`.

### D6. Placement-owned scenario validation

Route the host-loopback `i2pr-interop ntcp2 validate-scenario` invocation through `HostLoopbackDevelopmentPlacement.run(capture_stdout=True)`.

The host-loopback execution path must not directly call `subprocess.run()` or `subprocess.Popen()` for:

- i2pr preparation;
- scenario validation;
- i2pd inspect/listen/dial execution;
- i2pr listen/dial execution.

Non-host-loopback historical lanes may retain their existing implementation if changing them is unrelated to this defect.

### D7. Focused Plan 083/087 regression tests

Add focused tests proving:

1. placement capture returns parseable preparation output;
2. scenario validation is placement-owned;
3. the exported i2pd RouterInfo is copied byte-for-byte to `exchange/i2pd-router.info`;
4. the copied RouterInfo contains the exact configured NTCP2 endpoint;
5. the listener is alive when the dialer starts;
6. the dialer is reaped before the listener;
7. both processes are absent after cleanup;
8. `peer_router_info_invalid` before TCP produces the exact pre-protocol classification;
9. no-TCP records cannot use a protocol terminal result;
10. `reference-events-missing` cannot be emitted before authentic TCP progress;
11. a source SHA that does not equal the clean checked-out commit is rejected by the operator path or explicitly prevented by the execution procedure;
12. record validation rejects zero or placeholder executable, RouterInfo, placement, and record digests.

Prefer extending the existing Plan 083, Plan 086, Plan 087, and i2pd-driver test modules. Do not create a parallel runner or a second record schema.

### D8. Clean committed-head Plan 087 rerun

After D1-D7 are committed and the working tree is clean:

1. record the exact corrective source commit;
2. rebuild `target/debug/i2pr-interop` from that commit;
3. rebuild both instrumented and control i2pd drivers from the same pinned source and corrective driver source;
4. record all build/source/patch/library/binary digests;
5. run the structural RouterInfo verifier on both builds;
6. allocate a fresh run root and fresh peer state;
7. execute exactly one instrumented Plan 087 attempt;
8. preserve the compact record and bounded local logs;
9. do not automatically rerun on failure.

The instrumented attempt may proceed to the control comparison only when it passes every Plan 087 protocol criterion.

### D9. Control comparison

After an instrumented pass, execute one fresh-state control-build attempt with:

- the same corrective source commit;
- the same pinned i2pd revision;
- the same topology and endpoint policy;
- the same timeout policy;
- the same exact DeliveryStatus contract;
- no observer instrumentation;
- fresh identities, ports, data directories, and run root.

The control result must demonstrate externally visible success equivalent to the instrumented path and clean teardown. It must not rely on observer-only events for its pass decision.

### D10. Status and gate reconciliation

Update the planning/status authority only after the relevant evidence exists.

`plans/087-status.md` must:

- preserve the original diagnostic pre-TCP attempt as historical evidence;
- identify its non-canonical dirty-tree attribution limitation;
- record the corrective commit and rebuilt binary digests;
- record the structural RouterInfo verification result;
- record the fresh instrumented record digest;
- record the fresh control record digest;
- record exact Router Hash and DeliveryStatus correlation;
- record clean teardown;
- set `status = passed` only when all Plan 087 closure criteria are met.

`plans/088-status.md` must immediately stop claiming that Plan 086 is unclosed or that Plan 087 never executed. Before the Plan 087 pass it must accurately state:

```text
decision = insufficient-evidence
plan_086 = host-loopback-development-ready
plan_087 = open_pending_plan_090_reference_driver_correction
forward_attempt = authentic_pre_tcp_rejection_retained
plan_088 = blocked_pending_plan_087_pass
```

After Plan 087 passes, update the Plan 088 entry-gate section to `ready-for-reverse-execution` or the exact existing bounded equivalent without executing the reverse direction in this plan.

Update README, AGENTS, the NTCP2 interop skill, and the architecture documentation so the active sequence is:

```text
Plan 086 ready
  -> Plan 087 attempted and blocked pre-TCP
  -> Plan 090 corrective pass
  -> Plan 087 clean committed-head instrumented + control pass
  -> Plan 088 reverse execution
```

## Execution phases

### Phase 1: freeze the failure and trace ownership

1. Preserve the existing Plan 087 status and recorded digests.
2. Reproduce only the RouterInfo structural defect through the direct-driver inspect path, not through a full wire attempt.
3. Decode the exported RouterInfo using the repository decoder.
4. Capture the configured local address/port and the decoded address list.
5. Trace the pinned i2pd lifecycle and identify the first point where the address is absent.
6. Write the source-verification note.

Exit criterion:

```text
The exact lifecycle/config/export step responsible for the zero-address
RouterInfo is identified with pinned-source references and a deterministic
failing test.
```

Stop condition:

- If the address is present in the authoritative in-memory RouterInfo but absent only in the exported bytes, fix only export timing/source.
- If the address is never added, fix only configuration/order/update lifecycle.
- If the standalone comparison used materially different settings, correct the comparison before changing code.

### Phase 2: implement the narrow driver correction

1. Apply the smallest behavior-neutral change in the test-only driver.
2. Add a fail-closed endpoint verification before emitting `router_info_exported` success.
3. Rebuild instrumented and control binaries.
4. Run fresh and reused-data-directory structural tests.
5. Verify instrumented/control semantic parity.

Exit criterion:

```text
Both driver builds export a valid signed RouterInfo containing the exact
127.0.0.1:<configured-port> NTCP2 endpoint, and the listener binds that endpoint.
```

Stop condition:

- Do not continue if the exported endpoint differs from the bound listener.
- Do not continue if the fix requires serialized-byte mutation or pinned transport patches.
- Do not continue if only the instrumented build passes.

### Phase 3: correct evidence semantics and placement ownership

1. Add explicit parsing of the i2pr terminal record.
2. Map pre-TCP failures to pre-protocol terminal categories.
3. prohibit generic protocol classification without `tcp_connected`.
4. route scenario validation through placement.
5. add the focused regression tests.
6. rewrite the stale pre-execution portion of `plans/088-status.md`.

Exit criterion:

```text
A synthetic or fixture-driven peer_router_info_invalid result produces the
exact typed pre-protocol record, and every host-loopback subprocess is placement-owned.
```

### Phase 4: commit, rebuild, and run one instrumented attempt

1. Commit all corrective code and tests.
2. Verify the working tree is clean.
3. Rebuild all executed binaries.
4. verify measured binary digests match the invoked paths.
5. execute one fresh Plan 087 instrumented attempt.
6. preserve its record without automatic retries.

Outcomes:

- Passed: continue to Phase 5.
- Pre-TCP failure: Plan 090 remains open; do not execute Plan 088.
- TCP-authenticated protocol failure: preserve the result and follow Plan 087's bounded ownership procedure; do not execute Plan 088 until the forward direction passes.
- Ambiguous reference behavior: preserve the exact question and stop; do not relabel it as a pass.

### Phase 5: run one control comparison

1. Use a fresh run root and identities.
2. execute the control build once.
3. verify externally visible success equivalence.
4. verify clean teardown.
5. record the control digest.

Exit criterion:

```text
Instrumented and control forward attempts both satisfy the Plan 087 pass
contract without timeout/policy differences or observer-induced behavior changes.
```

### Phase 6: close Plan 087 and enable Plan 088

1. Update `plans/087-status.md` with exact committed-head evidence.
2. Update `plans/088-status.md` to reflect a satisfied forward gate.
3. update active-sequence documentation.
4. run all focused and repository boundary checks.
5. stop before reverse execution.

Exit criterion:

```text
Plan 087 status = passed
forward instrumented record digest != zero
forward control record digest != zero
source commit is exact and clean
reference revision is pinned
authenticated handshake completed
exact DeliveryStatus decoded
Router Hash correlation exact
cleanup clean
Plan 088 entry gate = ready
Plan 088 wire execution count = 0 in this plan
```

## Exact Plan 087 pass criteria

Plan 087 may close as `passed` only when the instrumented result proves:

```text
i2pd imported the exact i2pr RouterInfo
i2pd listener_ready is authentic and current-run scoped
i2pr connected to the intended 127.0.0.1 endpoint
both peers authenticated the NTCP2 session
SessionConfirmed was accepted
i2pr completed one authenticated frame write
i2pd authenticated and decrypted that frame
i2pd decoded exactly one DeliveryStatus message
delivery_status_message_id exactly matched
peer Router Hashes exactly matched on both sides
no duplicate primary DeliveryStatus was accepted
both processes exited or were boundedly terminated
no child or owned listener remained
cleanup_result = clean
```

The control comparison must then prove externally visible equivalent success and clean teardown.

The following are not passes:

- valid RouterInfo only;
- listener ready only;
- TCP connected only;
- handshake authenticated without the required data phase;
- observer-only instrumented success without control equivalence;
- a pass from a dirty tree or mismatched source SHA;
- any result with zero/placeholder provenance;
- any result requiring weakened i2pr RouterInfo validation.

## Required record fields for closure

The final Plan 087 status must bind at least:

```text
corrective_source_commit
reference_revision
reference_tree_sha256
driver_source_sha256
observer_patch_sha256
build_manifest_sha256
i2pr_binary_sha256
i2pd_instrumented_binary_sha256
i2pd_control_binary_sha256
instrumented_router_info_sha256
control_router_info_sha256
instrumented_router_hash_sha256
control_router_hash_sha256
instrumented_placement_record_sha256
control_placement_record_sha256
forward_instrumented_record_sha256
forward_control_record_sha256
delivery_status_message_id
instrumented_highest_stage_reached
control_external_terminal_result
cleanup_result
working_tree_clean = true
```

All SHA-256 fields must be lowercase 64-hex and nonzero. The source commit must be a real 40-hex commit present in repository history.

## Focused validation

Run focused checks first:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
```

Then run the repository's existing verification surface without adding new CI ceremony:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
git diff --check
```

The live Plan 087 command is run only after these checks pass from the committed corrective head.

## Small-model execution guidance

A smaller implementation model must follow these rules exactly:

1. Work one phase at a time. Do not modify Plan 083 record logic before first preserving a deterministic zero-address RouterInfo test.
2. Do not guess the i2pd lifecycle fix. Read the pinned 2.60.0 source and cite the exact functions controlling RouterInfo construction/update.
3. Prefer a one-function or one-ordering correction in the direct driver over new abstractions.
4. Do not touch production `crates/` transport code unless a new authentic post-TCP result proves an i2pr-owned protocol defect. The current defect is pre-TCP and reference-driver-owned.
5. Never alter `exact_ntcp2_address` to accommodate missing addresses.
6. Never manufacture a RouterInfo or RouterAddress in Python.
7. Keep the wrapper thin. Put orchestration in the existing canonical runner and placement classes.
8. Use existing schemas. Do not create Plan 090-specific probe records.
9. Treat the original Plan 087 attempt as diagnostic only. Do not copy its source attribution into the closure record.
10. Commit before the final live run, verify a clean tree, then use that exact commit SHA in the command and record.
11. Run one instrumented attempt. Do not automatically retry or patch after observing a failure.
12. Run the control comparison only after the instrumented pass.
13. Stop before Plan 088 execution even after all gates pass.
14. Do not claim completion when only unit tests pass; the clean committed-head instrumented and control records are mandatory.

## Failure handling

### Driver still exports zero addresses

- Preserve the failing structural test.
- Record the exact pinned-source lifecycle point.
- Keep Plan 090 open.
- Do not run the full Plan 087 wire attempt.

### RouterInfo contains an address but not the bound endpoint

- Treat as a driver lifecycle/configuration defect.
- Do not weaken endpoint matching.
- Keep Plan 090 open.

### Instrumented passes but control fails

- Treat as an instrumentation behavior-neutrality failure.
- Inspect observer patch and build divergence only.
- Do not proceed to Plan 088.

### Forward attempt reaches TCP and fails protocol

- Preserve the first authentic result.
- Follow Plan 087's one-reproduction ownership procedure.
- Do not classify it as this RouterInfo corrective pass succeeding unless the forward direction ultimately passes.

### Cleanup fails

- Record `cleanup_result != clean`.
- Treat closure as failed even if protocol exchange completed.
- Correct only owned process lifecycle behavior and rerun from fresh state after committing.

## Closure criteria

Plan 090 is complete only when every item is true:

- [ ] The zero-address defect is reproduced by a deterministic focused test.
- [ ] The pinned i2pd lifecycle/config/export ownership is documented with exact source references.
- [ ] The direct driver exports a signed RouterInfo containing the exact configured NTCP2 endpoint.
- [ ] Fresh and reused data-directory cases pass.
- [ ] Instrumented and control driver structural parity passes.
- [ ] The driver fails closed when its exported endpoint contract is not met.
- [ ] Plan 083 maps `peer_router_info_invalid` before TCP to the exact pre-protocol result.
- [ ] No pre-TCP result can serialize as generic `protocol_rejected` / `reference-events-missing`.
- [ ] Host-loopback scenario validation is placement-owned.
- [ ] Focused regression tests cover copy integrity, endpoint matching, concurrency, reaping, classification, and provenance.
- [ ] All corrective changes are committed before the live run.
- [ ] The working tree is clean for the live run.
- [ ] The invoked i2pr and i2pd binaries match recorded nonzero digests.
- [ ] One fresh instrumented Plan 087 attempt passes.
- [ ] One fresh control Plan 087 comparison passes.
- [ ] Exact Router Hash and DeliveryStatus correlations are recorded.
- [ ] Both attempts record clean teardown.
- [ ] `plans/087-status.md` records `status = passed` with exact evidence.
- [ ] `plans/088-status.md` no longer contains stale Plan 086/087 claims and records the forward gate as satisfied.
- [ ] README, AGENTS, skill, and architecture documentation reflect Plan 090 in the active sequence.
- [ ] Plan 088 has not been executed as part of this plan.
- [ ] NTCP2 remains experimental and non-advertised.

## Handoff summary

The implementation owner should begin in the Plan 064/076 direct-driver initialization path, not in production i2pr code. The first concrete artifact is a failing deterministic test that decodes the driver-written RouterInfo and proves the configured NTCP2 address is absent. After a source-verified, behavior-neutral driver correction, repair Plan 083's pre-TCP classification and placement-owned validation, commit everything, and execute one fresh instrumented Plan 087 attempt followed by one control comparison. Only a two-build forward pass with exact current-commit provenance authorizes updating the Plan 088 entry gate. The reverse probe itself belongs to Plan 088 and must not run during Plan 090.