# Plan 074 continuation amendment: Plan 081 pre-protocol correction

## Authority

- Date: 2026-08-01.
- Status: active continuation amendment.
- Applies to `plans/074-milestone-3-real-driver-and-constrained-host-corrective-roadmap.md`.
- Successor roadmap: `plans/081-milestone-3-pre-protocol-and-minimal-i2pd-corrective-roadmap.md`.

## Plan 074 implementation state

```text
Plan 075 = implemented
Plan 076 = implemented
Plan 077 = implemented
Plan 078 = closed on a pre-protocol failed attempt
Plan 079 = blocked
Plan 080 = implemented follow-up lane qualification and failed attempt record
```

Plan 074 correctly identified the need for a real reference process, real i2pd linkage, authentic structured events, and a constrained-host lane. Those prerequisites are now substantially present.

Its original transition from Plan 078 directly to Plan 079 is no longer active because the first Plan 078 attempt did not reach the protocol.

## Corrected continuation

The active continuation is:

```text
Plan 082 state preparation and runner contract correction
  -> Plan 083 minimal i2pr-to-i2pd probe
  -> Plan 084 reverse i2pd-to-i2pr probe and decision
  -> Plan 079 repeated validation only when admitted
```

Plan 081 does not invalidate the real Plan 076 driver or the qualified Plan 080 execution lane. It corrects the missing i2pr pre-protocol preparation/composition layer between them and live execution.

## Corrected environment interpretation

The Plan 074 roadmap originally treated Multipass as unreliable/unavailable on the constrained host. Plan 080 later established a usable owned Multipass path.

Active lane order for this work is therefore:

```text
1. reuse or refresh the existing Plan 080 Multipass lane
2. use the already selected Plan 076 i2pd driver
3. return to Plan 077 alternative lane selection only if the Plan 080 lane contract cannot be refreshed
```

Do not re-run the original Docker/QEMU/remote capability survey while the Plan 080 lane validates.

## Corrected protocol interpretation

The Plan 078 record does not establish a genuine protocol incompatibility. No retained event proves TCP connection, NTCP2 authentication, authenticated frame processing, or I2NP decode.

Future work must use the Plan 081/080 diagnostic correction:

```text
active blocker = pre-protocol launcher/harness contract defect
```

## Handoff

A model using Plan 074 as context must read these active successors before implementation:

```text
plans/067-active-sequence-amendment-plan-081.md
plans/081-milestone-3-pre-protocol-and-minimal-i2pd-corrective-roadmap.md
plans/082-i2pr-state-preparation-and-mixed-runner-contract-correction.md
plans/080-diagnostic-correction-amendment-plan-081.md
```

Plan 074 remains historical architecture context. Plan 081 is the current execution authority.