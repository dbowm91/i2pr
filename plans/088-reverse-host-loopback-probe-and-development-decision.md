# Plan 088: reverse host-loopback probe and development decision

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 085.
- Requires Plan 087 closed as passed with one genuine forward compact record and one behavior-neutral control-build comparison.
- Reuses the implemented Plan 084 reverse schema and runner.
- Blocks Plan 079 and conditionally gates Plan 072.
- Plan type: reverse-direction real protocol execution, bounded correction, and Milestone 3 development decision.

## Objective

Run the reverse authentic direction:

```text
i2pd initiator -> i2pr responder
```

Then issue the actual development decision based on one genuine passing or precisely localized record in each direction.

This plan answers:

```text
Can pinned i2pd 2.60.0 authenticate to the current i2pr responder and deliver one exact DeliveryStatus I2NP message over the selected development lane?
```

## Entry gate

Do not start unless `plans/087-status.md` binds:

```text
status = passed
instrumented_forward_record_sha256 = <nonzero>
control_forward_record_sha256 = <nonzero>
highest_stage_reached = i2np_delivery_status_decoded
cleanup = clean
```

Also require:

- same source commit or an explicitly recorded narrow forward correction commit;
- same pinned i2pd revision;
- current measured i2pr and i2pd binaries;
- Plan 086 `host-loopback-development-ready` or Plan 089 fallback-ready placement;
- fresh state, fresh run ID, fresh port, and fresh nonzero DeliveryStatus ID;
- no outstanding pre-TCP defect.

## Execution sequence

Execute exactly:

```text
validate active development placement
allocate fresh run root and loopback port
prepare i2pr responder state
prepare i2pd initiator state in inspect mode
validate both RouterInfos and Router Hashes
import exact i2pr RouterInfo into i2pd
freeze run identity and exact message ID
render and validate strict i2pr responder scenario
render strict i2pd initiator configuration
start real i2pr listener
wait for authentic i2pr listener_ready status
start real i2pd dialer
consume both current-run event streams
stop both processes in bounded order
verify cleanup
write one compact reverse record
```

Use the exact scenario ID:

```text
i2pd-to-i2pr-ipv4
```

No `-gen`, preparation, support, or fallback scenario ID may be used.

## Stage and event authority

Use the same ordered stage set as Plan 087:

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

### i2pr authority

Use real launcher/runtime status for:

- listener readiness;
- responder SessionRequest and SessionCreated processing;
- SessionConfirmed acceptance or rejection;
- authenticated link installation;
- authenticated frame read/decryption;
- exact DeliveryStatus decode;
- peer Router Hash and message ID correlation;
- terminal responder reason.

### i2pd authority

Use real direct-driver events for:

- exact i2pr RouterInfo import;
- TCP dial completion;
- initiator authentication;
- authenticated DeliveryStatus frame write completion;
- exact sent message ID;
- intended peer Router Hash.

No stage may be inferred from process lifetime, a listening socket, file existence, or a prior run.

## Outcome handling

### A. Passed

A pass requires:

```text
i2pr listener ready
i2pd imported exact i2pr RouterInfo
i2pd connected to intended endpoint
both sides authenticated the NTCP2 session
i2pr accepted SessionConfirmed and installed the link
i2pd completed one authenticated DeliveryStatus frame write
i2pr authenticated/decrypted the frame
i2pr decoded the exact DeliveryStatus ID
peer Router Hash correlation matched
cleanup was clean
```

After the instrumented pass, run one fresh-state control-build comparison.

Require equivalent external i2pr terminal success and clean teardown. The control build is not expected to emit observer-only events.

### B. Pre-TCP failure

Return to Plan 086/089 placement ownership. Plan 088 remains open. A pre-TCP failure cannot become a protocol decision.

### C. Real reverse defect

A real reverse defect requires at least `tcp_connected` and one bounded directly observed failing stage.

Reproduce once from fresh state before changing code.

Inspect by stage:

#### SessionRequest or SessionCreated

- responder static-key/IV binding;
- network ID;
- Noise transcript inputs;
- ephemeral/static agreement;
- timestamp/skew handling;
- options and padding lengths.

#### SessionConfirmed

- RouterInfo signature and identity hash;
- RouterInfo fragmentation and padding;
- SessionConfirmed part lengths and AEAD handling;
- static-key binding;
- RouterInfo verification and address selection.

#### Post-authentication data phase

- length obfuscation and SipHash state;
- nonce/counter direction;
- AEAD frame boundaries;
- block framing;
- short I2NP conversion;
- DeliveryStatus ID correlation.

Correct only the owning i2pr source path. Never patch i2pd acceptance or cryptographic behavior.

## Development decision

Write `plans/088-status.md` with exactly one decision.

### `two-way-development-probe-passed`

Requirements:

- Plan 087 forward direction passed;
- Plan 088 reverse direction passed;
- both instrumented records reached exact DeliveryStatus decode;
- both control-build comparisons were behavior-neutral;
- Router Hash and message ID correlation held in both directions;
- cleanup was clean;
- development-only topology limitations are explicit.

Consequence:

- Plan 079 becomes executable;
- Plan 072 remains inactive/optional;
- Java and release qualification remain pending;
- NTCP2 remains experimental and non-advertised.

### `one-way-passed-reverse-defect`

Use when the forward path passed but the reverse path has a reproducible, precisely localized i2pr-owned defect.

Required fields:

```text
highest_stage_reached
reason_code
owning_module_or_function
specification_section
first_record_sha256
reproduction_record_sha256
correction_commit_or_pending_action
```

Plan 079 remains blocked. Correct and rerun Plan 088.

### `ambiguous-reference-divergence`

Use only when:

- the reverse run reached the wire;
- disagreement is at one exact protocol stage;
- source and specification review do not identify ownership;
- observer/control behavior is neutral;
- Emissary would materially answer one precise question.

The status must write the exact question, role, stage, input artifact digest, and expected discriminating outcome.

Consequence:

- activate Plan 072 only for that question;
- keep Plan 079 blocked;
- do not create a general Emissary qualification lane.

### `manual-isolated-fallback-required`

Use only when the previously qualified direct development placement becomes non-executable before TCP for a demonstrated placement reason and Plan 089 has not yet been used.

Consequence:

- activate Plan 089;
- preserve all pre-TCP records;
- do not classify as protocol behavior.

### `insufficient-evidence`

Use when a real result cannot be reproduced deterministically after one bounded reproduction cycle and no safe ownership conclusion can be made.

Consequence:

- keep Plans 079 and 072 blocked unless a precise differential question exists;
- do not increase retries or timeouts without measurement.

Do not reuse the historical Plan 084 `lane-invalidated` token as the normal active development decision.

## Plan 079 handoff

Plan 079 becomes executable only when `plans/088-status.md` records exactly:

```text
decision = two-way-development-probe-passed
```

and binds:

```text
forward_instrumented_record_sha256
forward_control_record_sha256
reverse_instrumented_record_sha256
reverse_control_record_sha256
source_commit
reference_revision
placement_record_sha256
cleanup = clean
```

Plan 079 must treat the development loopback lane as protocol-development evidence only. Its existing no-public-network requirement must be satisfied by Plan 089 or another isolated lane before Level 2 closure. A Plan 086 host-loopback pass can unblock implementation continuation and protocol repetition work, but it cannot by itself satisfy release/isolation predicates.

## Plan 072 activation

Plan 072 may be activated only when `plans/088-status.md` records:

```text
decision = ambiguous-reference-divergence
```

and one exact diagnostic question.

Examples:

```text
Does pinned Emissary accept the same i2pr SessionConfirmed RouterInfo bytes that pinned i2pd rejects?
```

```text
Does pinned Emissary produce a first authenticated DeliveryStatus frame that i2pr decodes under the same role and keys where i2pd's frame is rejected?
```

## Deliverables

- one initial instrumented reverse record;
- one reproduction record for any real reverse failure;
- narrow corrective commit and focused tests when required;
- one passing instrumented reverse record or one precise unresolved stage record;
- one control-build comparison after a pass;
- `plans/088-status.md` with one exact decision;
- Plan 079/072 gate updates;
- concise active-status propagation.

## Acceptance criteria

Plan 088 closes only when:

- the reverse direction authentically reaches at least TCP;
- it either reaches exact DeliveryStatus decode or localizes a reproducible real wire-stage defect;
- no generic harness or evidence-finalization reason is used;
- a pass includes a behavior-neutral control comparison;
- cleanup is clean;
- one exact decision is recorded;
- Plan 079 and Plan 072 activation states are updated consistently;
- no release or production-support claim is made.

## Validation commands

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_reverse_probe.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-vectors.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Use the actual current reverse-test filename when the repository keeps reverse tests in `test_plan084.py` rather than a separate module. Record the exact live command and result digests in `plans/088-status.md`.

## Stop rules

Stop when:

- Plan 087 did not pass;
- placement or artifact provenance is invalid;
- the event streams cannot be bound to current processes;
- cleanup fails;
- a proposed correction changes i2pd semantics;
- failures become intermittent after one bounded reproduction;
- work expands into Plan 079 repetitions, Java, broad Emissary integration, performance testing, or production activation.

## Non-goals

Plan 088 does not:

- run the Plan 079 3/3 matrix;
- establish release isolation from the direct host-loopback lane;
- qualify Java;
- make Emissary mandatory;
- enable NTCP2 in normal operation;
- add recurring CI;
- redesign the compact record or runner architecture.

## Small-model execution guidance

1. Verify the exact Plan 087 passing record and source commit.
2. Run one instrumented reverse attempt.
3. Record the highest authentic stage before editing.
4. Reproduce one real failure once.
5. Correct one owning responder defect only.
6. Rerun until pass or one precise unresolved divergence.
7. Run the control build only after a pass.
8. Write one exact decision in `plans/088-status.md`.
9. Update gates; do not begin Plan 079 in the same commit.