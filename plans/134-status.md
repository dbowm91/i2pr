# Plan 134 status — Milestone 6 receive-window ACK ceiling closure

Status: **`ready-for-execution`**.

- Registered: **2026-08-27**.
- Source floor: `3d23c0eb698cbb366c8f7818a7b03bb95a00eb4f`.
- Plan of record:
  [`plans/134-m6-recv-window-ack-ceiling-closure.md`](134-m6-recv-window-ack-ceiling-closure.md).
- Plan 133 successfully closed the replay-evidence, Elligator reference, transactional-send, and authority defects it targeted. A subsequent audit discovered one new concrete Streaming receive-window defect: `RecvWindowPolicy::receive()` advances `highest_received` before rejecting `TooFarAhead`, allowing a dropped packet to contaminate a later `ackThrough`/NACK view.
- Plan 134 is therefore the **active narrow Milestone 6 local closure authority** until this defect is corrected and validated.
- This status does not reopen mixed-router interoperability as a local progression prerequisite. External acceptance debt remains separate.

## Current classification

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-and-plan133-final-gates
plan_132 = implementation-landed-evidence-superseded-by-plan133
plan_133 = passed-evidence-authority-reopened-by-plan134-concrete-streaming-defect
plan_134 = ready-for-execution

milestone6_local_product = corrective-reopened
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = hold-for-plan134-recv-window-ack-closure
next = Plan 134
```

## Narrow defect

At the Plan 133 source floor, the receive path currently performs:

```text
reject already-delivered duplicate
 -> update highest_received
 -> deliver if exactly next_expected
 -> reject if TooFarAhead
 -> otherwise buffer
```

This is incorrect for the rejected branch. A `TooFarAhead` packet is not accepted receive state and therefore must not update the sequence used to construct `ackThrough`.

Plan 134 requires:

```text
reject duplicate
 -> deliver accepted in-order packet
 OR
 -> check out-of-order admissibility
      TooFarAhead => return with receive/ACK state unchanged
 -> only accepted out-of-order packet updates highest_received and buffers
```

The existing receive-window boundary, ACK/NACK wire semantics, and all adjacent Streaming behavior must remain unchanged.

## Required focused evidence

Before this status may be changed to passed, executable tests must demonstrate:

1. a fresh-window `TooFarAhead` packet leaves `highest_received`, `ack_view`, `next_expected`, reorder state, and delivered count unchanged;
2. a rejected packet cannot inflate a previously valid accepted ACK ceiling;
3. a rejected packet cannot alter an existing legitimate reorder/NACK view;
4. the current `max_sequence - 1` accepted / `max_sequence` rejected boundary is preserved;
5. an extreme sequence such as `u32::MAX` remains bounded and ACK-state inert;
6. a manager-level packet-processing trajectory proves a rejected far-ahead sequence cannot appear in a later production outbound `ackThrough`.

## Required closure state

After all Plan 134 acceptance criteria and the full local validation gate pass, update this file to:

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-and-plan133-final-gates
plan_132 = implementation-landed-evidence-superseded-by-plan133
plan_133 = passed-evidence-authority-superseded-by-plan134-streaming-ack-closure
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

At closure, `plans/000-mvp-roadmap.md` must also be clarified so its historical independent-router/mixed-router Milestone 6 criteria remain explicit eventual MVP acceptance debt rather than being misread as a prerequisite for beginning Milestone 7 in the constrained development environment.

Do not create another Milestone 6 planning pass after Plan 134 unless a new concrete protocol defect is discovered.
