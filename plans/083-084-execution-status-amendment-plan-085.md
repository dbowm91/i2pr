# Plans 083-084 execution-status amendment: Plan 085

## Authority

- Date: 2026-08-04.
- Status: active status correction.
- Parent roadmap: Plan 085.
- Applies to:
  - `plans/083-minimal-i2pr-to-i2pd-ntcp2-wire-probe.md`;
  - `plans/083-status.md`;
  - `plans/084-i2pd-to-i2pr-reverse-probe-and-development-decision.md`;
  - `plans/084-status.md`;
  - any current documentation that describes Plan 084 as execution-closed.
- Historical documents remain preserved. This amendment changes active interpretation and execution authority only.

## Corrected state

```text
plan_083_schema = implemented
plan_083_runner = implemented
plan_083_reference_observer = implemented
plan_083_real_wire_execution = pending
plan_083_status = implementation-complete_execution-pending

plan_084_schema = implemented
plan_084_runner = implemented
plan_084_real_wire_execution = pending
plan_084_development_decision = pending
plan_084_status = implementation-complete_execution-pending

real_tcp_connection_attempt = 0
real_ntcp2_handshake_attempt = 0
real_authenticated_frame_attempt = 0
real_delivery_status_decode_attempt = 0
```

## Why the historical Plan 084 close is not active authority

Plan 084's plan-of-record requires:

- Plan 083 to pass first;
- a real reverse run to reach the wire;
- exact DeliveryStatus decode or a localized reproducible protocol failure;
- one exact development decision based on that wire result.

Those conditions were not met. The repository implemented the reverse schema, runner, tests, and static boundary checks, then recorded a host lane blocker before a wire attempt.

Therefore the historical values:

```text
lane_invalidation_pending
lane-invalidated
```

remain records of the environment assessment made at that time, but they do not satisfy Plan 084's execution acceptance criteria and cannot gate Plan 079.

The active interpretation is:

```text
implementation delivered
execution not attempted
protocol decision pending
```

## Historical Plan 080 distinction

Preserve both facts:

```text
historical_plan080_lane_qualification = valid_historical_record
current_plan080_guest_availability = unavailable_or_stale
```

Do not describe the guest as both currently qualified and currently executable without a fresh availability record.

A fresh Plan 080-style qualification is not required for the Plan 086 development-only host-loopback lane.

## Active successor ownership

Plan 083 implementation is consumed by Plan 087.

Plan 084 implementation is consumed by Plan 088.

```text
Plan 083 implementation -> Plan 087 execution authority
Plan 084 implementation -> Plan 088 execution and decision authority
```

Plans 083 and 084 remain useful implementation-history documents. They are not the next operator handoff documents.

## Status vocabulary

Use these exact active states:

```text
implementation-complete_execution-pending
host-loopback-development-ready
manual-isolated-fallback-required
passed
localized-protocol-defect
ambiguous-reference-divergence
insufficient-evidence
two-way-development-probe-passed
one-way-passed-reverse-defect
```

Do not introduce new active uses of:

```text
lane_invalidation_pending
lane-invalidated
```

The historical text may retain them with a clear historical qualifier.

## Required concise propagation

Plan 086 implementation must update current-status wording where necessary in:

```text
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
tests/integration/ntcp2/README.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
plans/030-milestone-3-closure.md or a dated amendment
plans/067-active-sequence-amendment-plan-081.md or a superseding amendment
```

Required meaning:

- Plan 082 is complete;
- Plan 083 and 084 implementation surfaces are complete;
- no real wire attempt has occurred;
- Plans 087 and 088 now own execution;
- Plan 079 remains blocked;
- Plan 072 remains conditional;
- NTCP2 remains experimental and non-advertised.

## Acceptance criteria

This amendment is satisfied when:

- Plan 085 is registered as active roadmap;
- Plan 086 is registered as next executable plan;
- current status no longer treats the historical Plan 084 lane decision as a protocol or development decision;
- Plan 079 gates on Plan 088;
- Plan 072 gates on Plan 088 ambiguity;
- historical Plan 083/084 implementation evidence remains intact;
- no interoperability result is fabricated.

## Handoff

The next implementation model must read:

```text
plans/085-milestone-3-host-loopback-development-execution-roadmap.md
plans/086-status-authority-and-host-loopback-development-lane.md
plans/083-084-execution-status-amendment-plan-085.md
```

It must execute Plan 086 only and stop before a complete peer connection.