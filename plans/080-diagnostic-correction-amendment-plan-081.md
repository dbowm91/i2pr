# Plan 080 diagnostic correction amendment: Plan 081 authority

## Status

- Date: 2026-08-01.
- Status: active interpretation amendment.
- Historical source: `plans/080-status.md` and commit `fe9d00b048757b4bf636e5f012e050e252a6bddb`.
- Successor authority: Plans 081-084.
- This file does not alter or delete Plan 080's historical records.

## Findings retained as valid

The following Plan 080 findings remain valid:

```text
owned Multipass guest was created
rootless sandbox capability passed inside the guest
no-public-network controls were installed and checked
source/cache/artifact transfer completed
Plan 076 i2pd driver artifacts were present and measured
i2pd produced a real signed RouterInfo
cleanup for the recorded attempt was clean
```

These findings qualify the execution environment and reference-driver preparation path. They should be reused by Plans 082-084.

## Diagnosis being corrected

Plan 080 recorded:

```text
plan080_typed_blocker = blocked-protocol-defect
plan080_first_failing_stage = i2pr_adapter_export_router_info
```

Subsequent source review shows that this is not a sufficient active diagnosis.

The live Plan 065 renderer requires nonzero 64-hex sender/receiver Router Hashes and a nonzero run-identity digest. The current mixed runner supplies empty strings for those fields. The reverse preparation path also constructs an unallowlisted `<primary-id>-gen` live scenario with empty strict fields.

Those inputs can fail before a live i2pr protocol process starts or before the adapter can produce/export RouterInfo. Broad exception translation then collapses the failure into `typed-harness-operation-failed`, and fallback process accounting can obscure whether process creation occurred.

The resource counters in Plan 080 remained at zero for handshakes, frames, and I2NP messages. No retained event proves TCP connection or any NTCP2 protocol stage.

## Active diagnosis

Use this diagnosis for all future planning and implementation:

```text
active_blocker_class = pre-protocol-launcher-harness-contract-defect
first_proven_missing_stage = authentic-i2pr-state-preparation-before-live-render
protocol_attempt_proven = false
protocol_pass_proven = false
protocol_defect_proven = false
```

The historical `blocked-protocol-defect` label remains in Plan 080 because it was the close status allowed by Plan 078 at that time. It must not be interpreted as proof that i2pr and i2pd exchanged or rejected NTCP2 bytes.

## Required correction

Plan 082 must:

1. add a dedicated test-only i2pr state-preparation command;
2. produce a signed endpoint-bound RouterInfo without opening sockets;
3. expose a verified Router Hash and RouterInfo digest;
4. prepare i2pd state through the existing Plan 076 driver;
5. freeze a canonical run identity;
6. render the strict live scenario with authentic nonzero values;
7. remove the `-gen` live-scenario path;
8. preserve precise failure categories;
9. make process counters reflect actual process creation.

Only after Plan 082 closes may a new live attempt begin.

## Environment reuse rule

Do not discard the Plan 080 lane solely because the protocol attempt was invalid.

Before reuse, verify:

```text
instance ownership/environment contract
source commit or transferred source digest
reference cache manifest
rootless probe result
no-public-network marker
current binary digests
```

When the preserved guest is unavailable, create a fresh owned guest through the existing Plan 080/Multipass lane. Do not start a new environment architecture project.

## Evidence correction rule

The Plan 080 direction record remains a valid failed-attempt record for process/environment auditing, but it is classified as:

```text
pre-protocol diagnostic only
not Level 1 protocol evidence
not a protocol rejection
not a pass
```

Plans 083-084 must use compact stage-based diagnostic records before reintegrating the broad historical evidence pipeline.

## Plan 079 effect

Plan 079 remains blocked, but its active dependency changes from a successful rerun of Plan 078 to the Plan 084 development decision.

Required unblock value:

```text
two-way-development-probe-passed
```

## Emissary effect

Plan 080 does not justify switching immediately to Emissary. The missing i2pr preparation contract would also block an Emissary peer.

Plan 072 remains conditional and may be activated only after a real wire-stage i2pd disagreement remains ambiguous.

## Closure

This amendment closes as a planning correction when:

- Plans 081-084 are registered;
- Plan 079 depends on Plan 084;
- future documentation distinguishes the valid lane qualification from the invalid protocol-defect inference;
- the next implementation pass begins with Plan 082 rather than another environment or reference-router build.