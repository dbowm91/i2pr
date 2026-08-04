# Plan 087: first real i2pr-to-i2pd host-loopback probe

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 085.
- Requires Plan 086 closed as `host-loopback-development-ready`, or Plan 089 to provide the same runner contract in an isolated manual container.
- Reuses the implemented Plan 083 forward schema, runner, state preparation, i2pd direct driver, and observer seams.
- Blocks Plan 088 and Plan 079.
- Plan type: one-direction real protocol execution with bounded corrective ownership.

## Objective

Produce the first authentic current-repository NTCP2 result:

```text
i2pr initiator -> i2pd responder
```

The run must use one fresh state pair, one exact nonzero DeliveryStatus message ID, and one real i2pr process plus one real pinned i2pd 2.60.0 process.

This plan answers:

```text
Can the current i2pr initiator authenticate to the pinned i2pd responder and deliver one exact DeliveryStatus I2NP message over the development loopback lane?
```

Plan 087 must prioritize a real wire result over additional harness architecture.

## Entry gate

Before starting any process, require:

```text
plan086_status = host-loopback-development-ready
or
plan089_status = manual-isolated-fallback-ready
```

Also require:

- clean repository or exact committed source SHA;
- current `target/debug/i2pr-interop` digest;
- current instrumented and control i2pd driver digests;
- pinned i2pd revision 2.60.0;
- valid Plan 082 preparation smoke;
- valid forward strict scenario using real Router Hashes;
- fresh run root;
- literal `127.0.0.1` endpoint when Plan 086 is used;
- network ID 99;
- no host-blocker environment variable forcing a synthetic lane-invalid record.

Do not start from a stale Plan 080 guest or historical qualification record.

## Execution sequence

Execute exactly:

```text
validate development lane
allocate fresh run root and loopback port
prepare i2pr state
prepare i2pd state in inspect mode
validate both RouterInfos and Router Hashes
copy exact peer RouterInfo to the expected exchange path
freeze run identity and exact message ID
render and validate strict i2pr initiator scenario
render strict i2pd responder configuration
start real i2pd listener
wait for authentic i2pd listener_ready event
start real i2pr dialer
consume both current-run event streams
stop both processes in bounded order
verify cleanup
write one compact forward record
```

Do not invoke the broad Plan 045/052 bundle or certificate finalization path.

## Stage authority

Use the existing ordered stage set:

```text
not_started
state_prepared
peer_router_info_imported
listener_ready
tcp_connected
noise_authenticated
session_confirmed_accepted
authenticated_frame_written
authenticated_frame_decrypted
i2np_delivery_status_decoded
```

A stage may advance only from authentic current-run events.

### i2pr authority

Use the real launcher status stream for:

- dialer process start;
- intended endpoint connection;
- initiator handshake state;
- authenticated link installation;
- authenticated frame write completion;
- terminal reason;
- expected peer Router Hash and DeliveryStatus counters.

### i2pd authority

Use the real direct-driver event stream for:

- exact i2pr RouterInfo import;
- listener readiness;
- peer authentication;
- post-AEAD frame observation;
- post-`FromNTCP2()` I2NP decode;
- exact DeliveryStatus message ID;
- authenticated peer Router Hash.

No file existence, process survival, port probe, or timeout expiry may promote a protocol stage.

## Required compact result

Reuse the Plan 083 forward record schema, with topology set to the active development lane.

Required fields include:

```text
schema
run_id
source_commit
direction = i2pr-to-i2pd-ipv4
reference = i2pd
reference_revision = 2.60.0
topology_kind
lane_or_placement_record_sha256
development_only = true
release_qualified = false
i2pr_binary_sha256
i2pd_binary_sha256
i2pr_router_info_sha256
i2pd_router_info_sha256
i2pr_router_hash_sha256
i2pd_router_hash_sha256
delivery_status_message_id
observed_events[]
highest_stage_reached
terminal_result
reason_code
process_counters
cleanup_result
record_sha256
```

When existing schema fields use a historical name such as `lane_qualification_sha256`, bind the Plan 086/089 placement record and document the development-only meaning. Do not fabricate an isolation qualification.

## Initial execution rule

Run one instrumented attempt without speculative corrections.

Preserve:

- compact record;
- exact command line after redaction of no secrets;
- binary and source digests;
- fixed stage/reason events;
- raw-local logs only under the disposable run root when explicitly enabled.

Do not rerun automatically.

## Outcome handling

### A. Passed

A pass requires all of:

```text
i2pd imported exact i2pr RouterInfo
i2pd listener ready
i2pr connected to intended 127.0.0.1 endpoint
both sides authenticated the NTCP2 session
SessionConfirmed accepted
i2pr completed one authenticated frame write
i2pd authenticated/decrypted the frame
i2pd decoded one DeliveryStatus
message ID exactly matched
peer Router Hash exactly matched
both processes exited or were cleanly terminated
no owned listening socket or child remained
```

After the instrumented pass, execute one fresh-state control-build comparison.

The control build cannot provide observer events. Require:

- i2pr externally visible terminal success equivalent to the instrumented run;
- same exact peer endpoint and message ID contract;
- no altered timeout or retry policy;
- clean teardown;
- no behavior regression caused by observer instrumentation.

Then Plan 087 closes as passed and Plan 088 becomes executable.

### B. Pre-TCP failure

Examples:

```text
prepare failed
RouterInfo validation failed
strict scenario failed
listener failed to start
listener-ready event missing
dialer process failed to start
TCP connect failed because listener endpoint was not present
```

These do not close Plan 087 as protocol defects.

Return to Plan 086 ownership, preserve the record, and correct only the demonstrated lane/configuration defect. Plan 087 remains open.

### C. Real protocol failure

A real protocol failure requires at least `tcp_connected`.

Allowed first-stage categories include:

```text
noise-session-request-rejected
noise-session-created-rejected
session-confirmed-rejected
peer-router-info-rejected
authenticated-link-install-failed
i2pr-frame-write-failed
i2pd-frame-authentication-failed
i2pd-i2np-decode-failed
delivery-status-id-mismatch
reference-events-missing-after_observed_peer_progress
```

For a real failure:

1. preserve the first record;
2. reproduce once from fresh state with unchanged binaries, ports policy, and timeout values;
3. confirm the same highest stage and bounded reason;
4. inspect the exact owning i2pr source and relevant specification section;
5. document the ownership conclusion;
6. implement one narrow correction only when ownership is clear;
7. rerun from fresh state;
8. stop when the direction passes or a precise unresolved reference divergence remains.

Do not close on a single non-reproduced timeout.

### D. Ambiguous reference divergence

Use only when:

- TCP was reached;
- the disagreement is at one exact protocol stage;
- i2pr and pinned i2pd source plus the specification do not identify ownership;
- observer/control behavior is neutral;
- a second implementation would materially answer the question.

Record the exact question for Plan 088. Do not activate Emissary from Plan 087 alone unless the active Plan 088 gate explicitly adopts it.

## Bounded correction scope

Permitted correction surfaces include the owning i2pr NTCP2 implementation for:

- SessionRequest/SessionCreated transcript construction or parsing;
- SessionConfirmed RouterInfo/padding/fragment handling;
- static-key, IV, or Router Hash binding;
- authenticated link installation;
- frame length obfuscation;
- AEAD nonce/counter direction;
- block framing and short-I2NP conversion;
- exact DeliveryStatus correlation.

A correction must include focused deterministic unit or vector coverage for the demonstrated defect.

Forbidden corrections:

- weakening RouterInfo signature or identity validation;
- disabling network ID checks;
- accepting unauthenticated frames;
- patching pinned i2pd behavior;
- synthesizing observer events;
- changing timeouts without stage-specific evidence;
- adding general retries;
- adding a new reference implementation;
- broad production activation.

## Deliverables

- one initial instrumented forward record;
- one reproduction record for any real failure;
- narrow corrective commit and tests when required;
- one passing instrumented record or one precise unresolved stage record;
- one control-build comparison after a pass;
- `plans/087-status.md`;
- concise active-status updates.

## Acceptance criteria

Plan 087 closes as passed only when:

- one authentic run reaches `i2np_delivery_status_decoded`;
- exact message ID and Router Hash correlation match;
- the record binds exact source, binaries, RouterInfos, and placement;
- the observer-instrumented result is authentic;
- one control-build run is behavior-neutral;
- cleanup is clean;
- no release or isolation claim is made;
- Plan 088 is registered as next.

Plan 087 may close as a localized unresolved defect only when:

- TCP was authentically reached;
- the same stage/reason reproduced once from fresh state;
- ownership and source/spec review are documented;
- a narrow correction was attempted when ownership was clear;
- the remaining question is precise;
- Plan 088 and Plan 079 remain blocked.

Plan 087 may not close on:

```text
lane_invalid
typed-harness-operation-failed
evidence-finalization-failed
invalid scenario
empty correlation fields
listener-only preflight
fake-process test success
```

## Validation commands

Use focused checks plus the exact live command:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
bash scripts/check-ntcp2-vectors.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Write the exact live command and record digest into `plans/087-status.md`.

## Stop rules

Stop and preserve evidence when:

- Plan 086/089 entry status is invalid;
- a binary or RouterInfo digest changes after run identity freeze;
- the event stream cannot be bound to the current process and run ID;
- cleanup fails;
- the proposed correction touches i2pd protocol semantics;
- the failure becomes intermittent after one controlled reproduction;
- work expands into reverse execution, repeated validation, Java, general Emissary integration, or release claims.

## Non-goals

Plan 087 does not:

- run the reverse direction;
- run a 3/3 matrix;
- run broad negative controls;
- prove external-network isolation;
- qualify Java;
- enable NTCP2 in production;
- add recurring CI;
- create a new runner or record architecture.

## Small-model execution guidance

1. Read Plan 086 status and verify the exact entry token.
2. Run one instrumented forward attempt.
3. Record the highest authentic stage before editing code.
4. Reproduce one real wire-stage failure once.
5. Correct one owning defect only.
6. Rerun until pass or one precise unresolved divergence.
7. Run the control build only after a pass.
8. Write `plans/087-status.md`.
9. Do not begin Plan 088 in the same commit.