# Plan 145 status — Milestone 7 remaining-gap corrective roadmap

Status: **`m7-corrective-umbrella-final-acceptance-open-via-plan151`**.

Registered: **2026-09-01**. Updated after Plan 151 registration: **2026-09-03**.

Plan of record:
[`plans/145-m7-sam31-remaining-gap-corrective-roadmap.md`](145-m7-sam31-remaining-gap-corrective-roadmap.md).

Newest execution/final-acceptance authority:
[`plans/151-status.md`](151-status.md).

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_146 = passed-m7-sam31-private-destination-reference-requalification
plan_147_raw_driver_implementation = landed-and-retained
plan_148 = blocked-audit-superseded
plan_149 = passed-m7-sam31-self-composing-local-product-corrective
plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = active-m7-sam31-final-acceptance-evidence-correction

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
milestone7_local_product = passed-via-plan149
milestone7_sam_localhost_final_acceptance = not-yet-closed
sam_independent_clients = at-least-two-passed-via-plan150
next_executable_plan = 151
next_product_layer = remain-on-milestone7
```

## Retained closed sub-claims

- Plan 146 private-destination reference compatibility is closed. Do not reopen without a concrete new incompatibility.
- Plan 147's owned raw socket handoff/byte pump, actual Streaming `Established` wait, OS CSPRNG runtime path, same-read preservation, and supervised ACK/retransmit driver remain retained implementation evidence.
- Plan 149's self-composing `SESSION CREATE` product path is passed and remains the local product-composition authority.
- Plan 150's exact pinned external-client core results remain valid evidence: two cross-client exact-byte directions, private destinations, SILENT, NAMING, negative cases, and a positive loopback FORWARD trajectory.

## Why the umbrella is not finally closed

The Plan 150 post-closure audit found that its final evidence ledger claimed
several required acceptance items without an executable case on the closing
lane. In particular, the external harness unconditionally marked a
`multiple-stream-lifecycle` row passed by referring to a Plan 149 sibling
suite that does not exist.

Plan 149 had explicitly deferred slow-peer, deterministic-fault, sibling and
broader lifecycle acceptance to Plan 150. Those requirements remained in Plan
150's acceptance criteria but were not all executed by the final harness.

Plan 151 therefore corrects final acceptance without discarding the working
architecture or valid external evidence.

## Corrective sequence

```text
146 passed
  -> 147 implementation retained
  -> 148 blocked historical audit
  -> 149 passed self-composing product
  -> 150 external core evidence retained
  -> 151 active final acceptance correction
```

Plan 151 owns:

- evidence-ledger integrity / no synthetic pass rows;
- sibling-stream isolation;
- slow-reader/slow-writer boundedness;
- DATA/ACK loss, duplicate, reorder, corruption, retransmit-ceiling cases beneath real SAM sockets;
- CLOSE/RESET/control-session lifecycle;
- complete FORWARD lifecycle/negative matrix;
- explicit focused Plan 127–134 regression execution;
- final current-head CI and hosted external-client workflow evidence.

## Environment policy

The remaining work remains compatible with the constrained environment:

```text
root/sudo              = not required
namespaces             = not required
Docker                 = not required
VM/Multipass           = not required
systemd                = not required
public I2P network     = not required
live NTCP2/SSU2        = not required
localhost TCP          = required
manual GitHub-hosted external-client workflow = allowed
```

## Handoff

Execute [`Plan 151`](151-m7-sam31-final-acceptance-evidence-correction.md)
only. Milestone 8 implementation remains blocked until `151-status.md`
explicitly records a passing closure.