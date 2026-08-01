# Plan 084: i2pd-to-i2pr reverse probe and development decision

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 081.
- Requires Plan 083 closed with either a pass or a corrected-and-rerun pass for `i2pr -> i2pd`.
- Requires Plan 082 authentic state preparation and strict live-scenario rendering.
- Blocks Plan 079.
- Plan type: reverse-direction development probe and continuation decision.

## Objective

Run the reverse genuine protocol direction:

```text
i2pd initiator -> i2pr responder
```

Then decide whether the current i2pr NTCP2 implementation has enough two-way development evidence to proceed to Plan 079 repeated validation.

This plan must answer:

```text
Can the pinned i2pd 2.60.0 initiator authenticate to the current i2pr responder and deliver one exact DeliveryStatus I2NP message?
```

## Preconditions

Before execution, confirm:

- Plan 082 preparation command and adapter remain valid;
- Plan 083 reached `i2np_delivery_status_decoded` after any required narrow correction;
- the Plan 080-style lane still validates;
- the current source commit and binaries are measured;
- the Plan 076 i2pd instrumented/control binaries are unchanged or explicitly rebuilt and re-measured;
- no outstanding pre-protocol generic failure remains.

Do not start the reverse direction merely because the environment exists.

## Execution architecture

Use the same narrow runner and diagnostic schema introduced by Plan 083, extended only for the reverse role assignment.

Required roles:

```text
i2pr = responder/listener
i2pd = initiator/dialer and DeliveryStatus sender
```

Required preparation order:

```text
validate lane
allocate fresh run root and endpoints
prepare i2pr state and export signed RouterInfo
prepare i2pd state and export signed RouterInfo
validate both identities and endpoints
import i2pr RouterInfo into i2pd
freeze run identity and exact message ID
render strict i2pr responder scenario
start i2pr listener
wait for real i2pr listener-ready status
start i2pd dialer
collect both structured event streams
shutdown and verify cleanup
write one compact diagnostic record
```

The i2pr responder scenario must use the exact primary ID `i2pd-to-i2pr-ipv4`. No `-gen` or preparation ID is allowed.

## Required observations

A passing reverse probe requires:

- i2pr listener-ready status from the real launcher;
- i2pd peer RouterInfo import;
- i2pd TCP connection to the intended i2pr endpoint;
- i2pd initiator authentication success;
- i2pr responder SessionRequest/SessionCreated/SessionConfirmed completion;
- authenticated link installation on i2pr;
- i2pd authenticated frame write completion;
- i2pr authenticated frame read/decryption;
- i2pr exact DeliveryStatus decode;
- exact nonzero message ID match;
- exact peer Router Hash match;
- clean process and lane teardown.

Use the i2pr launcher/runtime typed responder reasons when it rejects. Do not collapse responder-stage failures into one generic handshake failure.

## Stage model

Use the same ordered stage set as Plan 083:

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

For the reverse direction, event ownership changes but names remain stable.

## Pass predicate

Plan 084 reverse direction passes only when:

```text
lane contract valid
both prepared RouterInfos validate
i2pd imports the exact i2pr RouterInfo
i2pr listener is ready
i2pd authenticates to i2pr
i2pr accepts SessionConfirmed and installs the link
i2pd writes one authenticated DeliveryStatus frame
i2pr decrypts and decodes the exact DeliveryStatus ID
Router Hash correlation matches
cleanup is clean
```

After the instrumented i2pd path passes, run one control-build comparison. Require equivalent external i2pr terminal result and cleanup behavior. Observer-only fields are not expected from the control build.

## Failure ownership guide

### Before `tcp_connected`

Treat as preparation/configuration/placement ownership, not protocol failure. Return to Plan 082 or the narrow runner surface.

### At SessionRequest or SessionCreated

Inspect:

- endpoint/static-key/IV binding;
- network ID;
- Noise transcript inputs;
- ephemeral/static key agreement;
- clock/skew policy;
- padding lengths and options encoding.

### At SessionConfirmed

Inspect:

- RouterInfo bytes and signature;
- RouterIdentity hash continuity;
- static-key binding;
- SessionConfirmed part-one/part-two length and AEAD processing;
- responder RouterInfo verification;
- fragmentation/padding handling.

### After authentication but before frame decode

Inspect:

- frame length obfuscation;
- SipHash IV/key evolution;
- AEAD nonce/counter direction;
- block framing and bounds;
- short I2NP header conversion;
- DeliveryStatus message ID correlation.

A real protocol failure must be reproduced once from fresh state before a correction is accepted.

## Development decision

Create `plans/084-status.md` with one of the following exact decisions.

### `two-way-development-probe-passed`

Requirements:

- Plan 083 passed;
- Plan 084 reverse direction passed;
- both control-build comparisons are behavior-neutral;
- exact message ID and Router Hash correlation hold;
- cleanup and lane proofs hold.

Consequence:

- Plan 079 becomes executable;
- Emissary remains optional/not activated;
- Java and release qualification remain pending;
- NTCP2 remains experimental and non-advertised.

### `one-way-passed-reverse-defect`

Use when Plan 083 passed but the reverse direction has a real reproducible owned defect.

Required status content:

- highest stage reached;
- exact bounded reason;
- owning i2pr module/function;
- source/spec interpretation;
- narrow correction plan or commit;
- rerun requirement.

Plan 079 remains blocked until the reverse direction passes.

### `same-stage-two-way-i2pr-defect`

Use when both directions fail at the same i2pr-owned semantic stage after reaching the wire.

Consequence:

- create one narrow protocol/runtime corrective plan;
- do not activate Emissary merely for corroboration when ownership is already clear;
- rerun Plans 083 and 084 after correction.

### `ambiguous-reference-divergence`

Use only when:

- the wire is reached;
- i2pr and i2pd disagree at a precise stage;
- pinned source and specification review do not identify ownership;
- observer/control behavior is neutral;
- a second implementation would materially answer the question.

Consequence:

- activate Plan 072 for that exact role/stage only;
- pin one Emissary revision;
- do not build a general Emissary qualification lane.

### `lane-invalidated`

Use only when the existing lane's ownership, no-public-network, or artifact-binding proof actually fails before protocol execution.

Consequence:

- return to the existing Plan 077/080 lane scripts;
- do not redesign the environment unless the existing lane cannot be refreshed.

## Differential use of Emissary

Emissary is not required when:

- both i2pd directions pass;
- a failure is clearly owned by i2pr source/spec review;
- the failure remains pre-protocol;
- the only uncertainty is evidence finalization.

Emissary may be activated only with a written question such as:

```text
Does Emissary accept the same i2pr SessionConfirmed RouterInfo that i2pd rejects?
```

or:

```text
Does Emissary produce a first authenticated data frame that i2pr decodes where i2pd's equivalent frame fails?
```

The result is diagnostic, not release authority.

## Diagnostic record

Reuse `i2pr-minimal-i2pd-probe-v1` or bump it only when role-neutral fields cannot express the reverse direction.

Required direction value:

```text
i2pd-to-i2pr-ipv4
```

Required process counters:

```text
i2pr_prepare
i2pd_prepare
i2pr_listener
i2pd_dialer
```

Required exact correlation fields remain the same as Plan 083.

## Acceptance criteria

Plan 084 closes only when:

- a real reverse-direction protocol run reaches the wire;
- it either reaches exact DeliveryStatus decode or localizes a real reproducible protocol failure;
- no generic pre-protocol or evidence-finalization reason is used as the terminal diagnosis;
- observer/control comparison is completed after a pass;
- cleanup is clean;
- the development decision is recorded using one exact allowed value;
- Plan 079 status/dependency is updated accordingly;
- Plan 072 is activated only for a specific unresolved differential question;
- no release or production-support claim is made.

## Validation commands

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Record the exact live command and result digest in `plans/084-status.md`.

## Stop rules

Stop when:

- Plan 083 has not passed;
- the strict responder scenario cannot be rendered from Plan 082 state;
- lane or artifact provenance is invalid;
- a failure remains pre-protocol;
- the reference observer changes protocol behavior;
- the proposed fix requires changing pinned i2pd protocol semantics;
- work expands into Java, full Emissary integration, repeated 3/3 validation, or production activation.

## Non-goals

Plan 084 does not:

- perform Plan 079 repetitions or negative controls;
- qualify Java;
- make Emissary mandatory;
- close release qualification;
- enable NTCP2 in the daemon;
- add CI or performance testing;
- redesign the evidence architecture.

## Small-model execution guidance

1. Confirm Plan 083 passed and record its exact artifacts.
2. Extend the same narrow runner for the reverse role assignment.
3. Do not create a second runner architecture.
4. Execute one instrumented reverse attempt.
5. Stop at the first real failing stage and inspect that owner.
6. Reproduce once before changing code.
7. Run the control build only after the instrumented path passes.
8. Write the exact development decision.
9. Update Plan 079 only after the decision is final.