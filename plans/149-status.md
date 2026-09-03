# Plan 149 status — SAM 3.1 self-composing local product corrective

Status: **`passed-m7-sam31-self-composing-local-product-corrective`**.

Registered: **2026-09-02**. Closed: **2026-09-02** (UTC). Authority note updated: **2026-09-03**.

Plan of record:
[`plans/149-m7-sam31-self-composing-local-product-corrective.md`](149-m7-sam31-self-composing-local-product-corrective.md).

Newest final Milestone 7 acceptance authority:
[`plans/151-status.md`](151-status.md).

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_146 = passed-m7-sam31-private-destination-reference-requalification
plan_147_raw_driver_implementation = landed-and-retained
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
next_executable_plan = 151
```

## What Plan 149 closed

Plan 149 fixed the product-composition blocker discovered after Plan 147/148.
A successful SAM `SESSION CREATE` now composes the supported localhost STREAM
product transactionally before returning success.

Retained implementation/result:

- one `Arc<DestinationIdentity>` allocation shared by the destination runtime and SAM bridge; no second private identity reconstruction;
- `SamLocalProductFabric` creates signed LeaseSet2, outbound role, and a localhost inbound-tunnel provider with OS CSPRNG runtime material;
- `DestinationRuntime::with_shared_identity` preserves one secret ownership graph;
- `SamDestinationBridge` and the inbound delivery factory are installed automatically;
- one per-destination runtime driver is spawned under explicit `ChildScope`/cancellation ownership;
- local peer LeaseSet2 is resolved/validated through `SamDestinations::resolve_local_lease_set2`, without test-side cross-installation;
- `bridge_to_peer` preserves canonical peer routing across deliveries;
- missing/failing local delivery is surfaced through typed `DeliverySweepCounters` rather than silently dropped;
- CONNECT/ACCEPT raw transition honors `SILENT`; non-silent ACCEPT emits authenticated peer public Destination metadata;
- same-read STREAM command newline + initial raw bytes are preserved.

## Canonical black-box evidence

`crates/i2pr-daemon/tests/sam_stream_self_composed.rs` is the Plan 149 product-composition authority.

After listener startup it drives behavior through SAM TCP/raw bytes only and
does not call private bridge/LeaseSet2/tunnel-factory/driver/delivery helpers.

Its four tests prove:

1. self-composed CONNECT/ACCEPT with exact bidirectional 2 MiB transfer;
2. `SILENT=true` raw-first behavior;
3. clean session/control teardown and registry baselines;
4. same-read command + raw bytes, non-silent status, and authenticated ACCEPT peer metadata.

The 2 MiB hosted-runner bound was widened to 120 seconds because the balanced
Streaming profile deliberately uses its reference delayed-ACK behavior. That
change preserved protocol timing/window semantics rather than accelerating the
wire behavior solely for CI.

## What Plan 149 did not close

Plan 149 deliberately carried the following acceptance work forward rather
than claiming it:

- two simultaneous sibling-stream isolation;
- explicit slow-reader/slow-writer pressure cases;
- DATA-drop retransmission;
- ACK-drop recovery;
- duplicate DATA exact-once behavior;
- reordered DATA recovery;
- authenticated/ciphertext corruption rejection;
- retransmission-ceiling terminal behavior;
- broader CLOSE/RESET lifecycle matrix;
- final external-client/FORWARD/NAMING acceptance.

Plan 150 produced valid external-client core evidence, but its final evidence
audit found several of the above were not actually executed despite being
marked/treated as passed. Plan 151 now owns those remaining final-acceptance
items.

## Plan 149 closing validation

The Plan 149 closing pass recorded successful workspace format/check/test,
focused SAM tests, clippy/doc, static boundary checks, NTCP2 harness regression,
and cargo-deny policy checks. The canonical Plan 149 product suite passed four
tests.

These remain historical closing evidence for product composition. Plan 151
must rerun the appropriate current regression floor on its closing head.

## Handoff

Do not reopen Plan 149 architecture without a concrete defect. Execute
[`Plan 151`](151-m7-sam31-final-acceptance-evidence-correction.md) to close the
remaining acceptance/evidence debt. Milestone 8 implementation remains blocked
until Plan 151 passes.