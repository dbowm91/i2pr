# Plan 083: minimal i2pr-to-i2pd NTCP2 wire probe

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 081.
- Requires Plan 082 closed with authentic prepared state and a valid strict live scenario.
- Requires Plan 076 real i2pd driver artifacts and a valid Plan 080-style owned Multipass lane.
- Blocks Plan 084 and Plan 079.
- Plan type: one-direction development diagnostic.

## Objective

Run the first genuine protocol attempt in this sequence:

```text
i2pr initiator -> i2pd responder
```

Use one exact nonzero DeliveryStatus message ID and report the highest authentic protocol stage reached.

This plan must answer one narrow question:

```text
Can the current i2pr initiator authenticate to the pinned i2pd 2.60.0 responder and deliver one exact DeliveryStatus I2NP message?
```

It must not attempt repeated validation, negative controls, Java, Emissary, release evidence, or broad harness reconciliation.

## Why this direction is first

This direction avoids the previous i2pr responder preparation cycle and exercises:

- i2pr outbound TCP connection;
- i2pr initiator Noise/NTCP2 state machine;
- i2pd inbound NTCP2 responder;
- i2pr SessionConfirmed construction and RouterInfo submission;
- i2pr authenticated frame write;
- i2pd AEAD verification and I2NP conversion.

The Plan 076 observer provides a strong receiver-side terminal event after authenticated decryption and exact DeliveryStatus decode.

## Execution architecture

Use:

```text
one owned Plan 080-style Multipass guest
one rootless sealed single-network namespace inside the guest
one real target/debug/i2pr-interop process
one real Plan 076 i2pd instrumented driver process
IPv4 synthetic endpoints only
network ID 99
no public interface or route
fresh mutable state
```

Reuse the existing guest when its ownership and environment manifest validate. Otherwise create a fresh owned guest through the existing Plan 048/049/050/080 lane scripts. Do not design a new environment.

## Minimal probe boundary

Create a focused runner or mode, recommended:

```text
tests/integration/ntcp2/harness/minimal_i2pd_probe.py
```

or:

```text
scripts/interop/run-minimal-i2pd-ntcp2-probe.py
```

The runner should orchestrate only:

1. lane/placement validation;
2. i2pr state preparation from Plan 082;
3. i2pd state preparation through Plan 076;
4. RouterInfo exchange and strict validation;
5. run-identity freeze;
6. i2pd listener start;
7. i2pr dial start;
8. structured event collection;
9. one exact DeliveryStatus transfer;
10. bounded shutdown and cleanup;
11. one compact diagnostic record.

Do not route this first attempt through the complete Plan 045/052 bundle finalization path.

## Required startup order

```text
validate lane ownership and no-public-network state
allocate fresh run root and endpoints
prepare i2pr state
prepare i2pd state
validate both RouterInfos and Router Hashes
freeze run identity and message ID
render strict i2pr initiator scenario
render strict i2pd responder config
start i2pd listener
wait for real i2pd listener-ready event
start i2pr dialer
consume i2pr terminal status and i2pd events
stop both processes
verify cleanup
write diagnostic record
```

A TCP port probe may be used only as a bounded liveness aid after the real i2pd listener-ready event. It is never protocol evidence.

## Stage model

The result record must contain one `highest_stage_reached` value from this ordered set:

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

Suggested terminal results:

```text
passed
protocol_rejected
protocol_timeout
pre_protocol_rejected
cleanup_failed
lane_invalid
```

Suggested fixed reasons include:

```text
i2pd-listener-not-ready
i2pr-dial-start-failed
tcp-connect-failed
noise-session-request-rejected
noise-session-created-rejected
session-confirmed-rejected
peer-router-info-rejected
authenticated-link-install-failed
i2pr-frame-write-failed
i2pd-frame-authentication-failed
i2pd-i2np-decode-failed
delivery-status-id-mismatch
reference-events-missing
cleanup-verification-failed
```

Use the exact existing launcher/reference reason when it is already bounded and stable. Do not create duplicate aliases unnecessarily.

## Event authority

### i2pr authority

Use the real launcher terminal/status records for:

- dial process start;
- handshake attempt;
- authenticated link establishment;
- frame write completion;
- i2pr terminal reason;
- message ID and expected peer Router Hash counters.

### i2pd authority

Use the Plan 076 real driver events for:

- RouterInfo import;
- listener readiness;
- connection/session authentication where emitted;
- successful asynchronous frame write callback where applicable;
- post-AEAD authenticated frame observation;
- post-`FromNTCP2()` I2NP decode;
- exact DeliveryStatus ID and peer Router Hash.

### Cross-process correlation

A passing record requires exact agreement on:

```text
run_id
i2pr Router Hash
i2pd Router Hash
delivery_status_message_id
direction
reference binary digest
```

No event from a prior process or prior run may satisfy the current record.

## Pass predicate

Plan 083 passes only when all are true:

```text
lane contract valid
i2pr state prepared and validated
i2pd state prepared and validated
i2pd imported i2pr RouterInfo
i2pd listener became ready
i2pr connected to the intended endpoint
i2pr NTCP2 handshake completed
i2pd accepted/authenticated the session
i2pr completed one authenticated frame write
i2pd observed successful AEAD decryption
i2pd converted one I2NP DeliveryStatus message
decoded message ID equals the planned nonzero ID
peer Router Hash correlation matches
both processes stop cleanly
no residual owned namespace/process/socket/state remains
```

The uninstrumented i2pd control build is not required for the first failed attempt. When the instrumented direction passes, run the same direction once with the control build and require the externally visible i2pr terminal result and cleanup result to remain equivalent. The control run cannot provide observer events; it is a behavior-neutrality check, not the primary proof.

## Failure policy

When the probe fails:

1. preserve the compact failed diagnostic record;
2. preserve raw-local logs only inside the disposable guest when explicitly enabled;
3. identify the first directly observed divergent stage;
4. inspect the owning i2pr/i2pd source path;
5. reproduce once from fresh state with unchanged binaries and timeouts;
6. create or apply only a narrow owning correction;
7. rerun from fresh state after any code change.

A protocol failure is not an environment blocker when the lane remains valid.

A pre-protocol failure means Plan 082 is incomplete and Plan 083 remains open.

## Diagnostic record

Recommended schema:

```text
i2pr-minimal-i2pd-probe-v1
```

Required fields:

```text
schema
run_id
source_commit
direction = i2pr-to-i2pd-ipv4
reference = i2pd
reference_revision
lane_qualification_sha256
topology_kind
parent_network_state_unchanged
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

`observed_events[]` should contain only fixed event names, source side, and event-record digest. Do not copy raw protocol payloads.

## Process accounting

Record preparation and live processes separately:

```text
process_counters.i2pr_prepare
process_counters.i2pd_prepare
process_counters.i2pd_listener
process_counters.i2pr_dialer
```

Each entry contains:

```text
started
exited
forced
```

No counter is inferred from a later file or event.

## Suggested touched files

```text
tests/integration/ntcp2/harness/minimal_i2pd_probe.py
or scripts/interop/run-minimal-i2pd-ntcp2-probe.py
tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py
tests/integration/ntcp2/harness/test_plan083.py
scripts/check-ntcp2-interoperability.sh only for narrow invariants
plans/083-status.md
```

Use existing adapters and event validators. Do not fork their protocol parsing.

## Acceptance criteria

Plan 083 closes when one of the following is true.

### Passed

- one fresh-state instrumented run reaches `i2np_delivery_status_decoded`;
- exact message ID and Router Hash correlation match;
- one control-build run has equivalent externally visible behavior and clean shutdown;
- the lane remains no-public-network and artifact-bound;
- no broad evidence-finalization failure hides the result;
- `plans/083-status.md` records exact commands and record digests.

### Real protocol defect localized

- the run reaches at least `tcp_connected`;
- the first failed protocol stage is directly observed and reproducible once from fresh state;
- the owning source area is identified;
- the exact result and reproduction are recorded;
- no pass is claimed;
- Plan 084 remains blocked until the owning correction is completed and Plan 083 reruns.

Plan 083 may not close on `typed-harness-operation-failed`, empty hashes, an invalid scenario, or evidence-finalization failure.

## Validation commands

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

The live probe command must be written exactly into `plans/083-status.md` when executed.

## Stop rules

Stop when:

- Plan 082 authentic preparation does not pass;
- the lane ownership/attestation record is invalid;
- either binary digest differs from the prepared run identity;
- i2pd events cannot be bound to the current driver binary and run ID;
- a proposed correction requires patching i2pd protocol behavior;
- work expands into Java, Emissary, repeated validation, public NetDB, or daemon activation.

## Non-goals

Plan 083 does not:

- run the reverse direction;
- run 3/3 repetitions;
- run negative controls;
- use Java or Emissary;
- close Plan 079 or Plan 073;
- rework the environment architecture;
- produce release evidence;
- enable production NTCP2.

## Small-model execution guidance

1. Verify Plan 082 status and run its pre-protocol self-check.
2. Implement the compact probe record and tests using fake event streams.
3. Reuse the existing real i2pd driver adapter.
4. Execute one instrumented live attempt.
5. Stop at the first real failing stage.
6. Do not add retries or broaden timeouts before reading the owning event/source.
7. Run the control build only after the instrumented path passes.
8. Write `plans/083-status.md` before starting Plan 084.