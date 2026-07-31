# Plan 070 supersession amendment: execute Plan 074 sequence instead

## Status

- Plan 070 is superseded as active execution authority.
- Historical Plan 070 remains unchanged as a record of the intended first i2pd two-way execution.
- Active replacement sequence: Plans 075, 076, 077, then 078.

## Reason

Plan 070 cannot be executed truthfully from the current repository state because:

1. the Plan 064 i2pd helper does not link real pinned i2pd library targets and its listen/dial paths remain terminal stubs;
2. the implemented Plan 069 runner launches i2pr for both process handles rather than one i2pr and one real i2pd process;
3. the runner may infer authenticated/data milestones without authentic reference structured events;
4. active provenance contains synthetic fallback behavior that cannot support a real pass;
5. the current host cannot provide the rootless network namespace assumed by earlier work, and Multipass is not a reliable recovery lane.

A rootless-capable host alone would not correct items 1 through 4.

## Required replacement order

```text
Plan 075: correct runner process roles, event authority, and provenance
Plan 076: build/link genuine pinned i2pd and implement real inspect/listen/dial
Plan 077: select and prequalify Docker --network none, QEMU -nic none, or remote lane
Plan 078: execute the first real two-way i2pd run
```

Do not create `plans/070-status.md` or mark Plan 070 blocked/completed from the current state. Closure evidence belongs to the replacement plans.

## Preserved intent

Plan 078 preserves Plan 070's valid intended outcome:

- one real pass in each i2pd direction;
- exact DeliveryStatus and Router Hash correlation;
- behavior-neutral observer control;
- bounded failure correction;
- no Level 3 claim.

It removes the false prerequisites and adds explicit driver/runner/lane qualification.