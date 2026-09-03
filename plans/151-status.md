# Plan 151 status — Milestone 7 SAM 3.1 final acceptance evidence correction

Status: **`active-m7-sam31-final-acceptance-evidence-correction`**.

Registered: **2026-09-03**.

Plan of record:
[`plans/151-m7-sam31-final-acceptance-evidence-correction.md`](151-m7-sam31-final-acceptance-evidence-correction.md).

## Current authority

Plan 149 remains the passed self-composing localhost SAM product authority.
Plan 150 retains successful external-client core evidence, but its broad final
closure interpretation is superseded by Plan 151 because several acceptance
items were recorded as passed without an executable test on the closing lane.

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_146 = passed-m7-sam31-private-destination-reference-requalification
plan_147_raw_driver = landed-and-retained
plan_148 = blocked-audit-historical-superseded
plan_149 = passed-m7-sam31-self-composing-local-product-corrective

plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = active-m7-sam31-final-acceptance-evidence-correction

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
milestone7_local_product = passed-via-plan149
milestone7_sam_localhost_final_acceptance = not-yet-closed
sam_independent_clients = at-least-two-passed-via-plan150
router_to_router_interoperability = not-claimed

next_executable_plan = 151
next_product_layer = remain-on-milestone7
```

## Retained Plan 150 evidence

Do not discard these successful results while executing Plan 151:

- routine CI and the manual SAM external-client workflow passed on the audited Plan 150 head;
- pinned i2psam and the qualified pinned i2plib SAM surface exchanged exact 2 MiB payloads in both cross-client directions;
- private-destination import/generation passed through both counted clients;
- SILENT transcript evidence passed;
- NAMING and negative-input matrices passed;
- positive STREAM FORWARD to a real loopback target passed;
- official libsam3 was built/probed and correctly not counted because its public API rejects i2pr's compact Ed25519 `PRIV` shape;
- Plan 149's self-composed black-box product path remained green.

## Why final closure is reopened

The Plan 150 harness currently records at least one required result as passed
without executing the acceptance case:

```text
record "multiple-stream-lifecycle" passed "retained Plan 149 black-box sibling/lifecycle suite"
```

The referenced Plan 149 black-box suite has four tests and does not contain a
two-sibling-stream isolation test.

Plan 149 explicitly deferred the slow-reader/slow-writer, fault matrix, and
sibling-stream acceptance items to Plan 150. Plan 150's acceptance criteria
retain those requirements, but the current Plan 150 harness does not execute
them before generating its final evidence summary.

The positive FORWARD lane is also useful but narrower than the full Plan 150
FORWARD lifecycle/negative matrix.

Therefore Plan 150 remains valid external-client core evidence but is no
longer the final Milestone 7 closure authority.

## Plan 151 required closure areas

Plan 151 must add executable evidence for:

1. evidence-ledger integrity / no synthetic pass rows;
2. two simultaneous sibling STREAMs and close-one/keep-one isolation;
3. slow-reader boundedness;
4. slow-writer/reverse-pressure boundedness;
5. DATA-drop retransmission;
6. ACK-drop recovery;
7. duplicate DATA exact-once application delivery;
8. DATA reorder -> ordered application delivery;
9. authenticated/ciphertext corruption rejection;
10. retransmission-ceiling bounded terminal behavior;
11. CLOSE/RESET/control-session lifecycle cleanup;
12. full FORWARD lifecycle/negative matrix;
13. explicit focused Plan 127–134 regression execution;
14. final external-client rerun and hosted workflow on the exact closing head.

## Handoff

Execute `plans/151-m7-sam31-final-acceptance-evidence-correction.md`.

Do not begin Milestone 8 implementation until this status is replaced by an
explicit passing Plan 151 closure record backed by executable evidence for
every required final acceptance row.