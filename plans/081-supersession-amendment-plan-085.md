# Plan 081 supersession amendment: Plan 085 execution recovery

## Authority

- Date: 2026-08-04.
- Status: active supersession amendment.
- Applies to `plans/081-milestone-3-pre-protocol-and-minimal-i2pd-corrective-roadmap.md` and its Plans 082-084 sequence.
- Successor authority: Plan 085 and `plans/067-active-sequence-amendment-plan-085.md`.
- Historical Plan 081 documents and implementation records remain preserved.

## Closure of Plan 081 implementation objectives

Plan 081 successfully produced:

- Plan 082 authentic pre-protocol state preparation;
- real RouterInfo and Router Hash correlation;
- strict scenario validation without socket side effects;
- Plan 083 forward compact schema and real-subprocess runner;
- Plan 084 reverse compact schema and real-subprocess runner;
- bounded i2pd authentication and I2NP observer waits;
- focused fail-closed test coverage.

These implementation objectives are complete.

## Unfinished execution objective

Plan 081 did not produce:

```text
real tcp_connected event
real NTCP2 authentication event
real authenticated frame transfer
real DeliveryStatus decode
real two-way development decision
```

The historical Plan 084 lane blocker does not satisfy Plan 084's wire-execution closure criteria.

## Successor scope

Plan 085 owns the remaining execution work:

```text
Plan 086 development lane
Plan 087 forward real probe
Plan 088 reverse real probe and decision
Plan 089 conditional isolated fallback
```

Plan 081 must not be reopened for further schema, runner, or environment-framework expansion.

## Handoff rule

Any model directed to continue Plan 081 must instead read:

```text
plans/085-milestone-3-host-loopback-development-execution-roadmap.md
plans/067-active-sequence-amendment-plan-085.md
```

The next executable plan is Plan 086.