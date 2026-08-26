# Plan 131 status — Milestone 6 final local correctness closure

Status: **`ready-for-execution`**

- Registered: **2026-08-26**.
- Source floor: `8f2b3dfe44b480beb7f411613b8d7089aaeb7a19`.
- Plan of record: [`plans/131-m6-final-local-correctness-closure.md`](131-m6-final-local-correctness-closure.md).
- This status is the **current Milestone 6 closure authority**. The `passed-milestone6-final-wire-runtime-corrective-closure` result in `plans/130-status.md` remains historical evidence for the work that landed in Plan 130, but its Milestone 6 closure decision is reopened by Plan 131.

## Reopen reasons

Plan 131 is limited to four remaining local correctness/evidence gaps discovered after auditing the landed Plan 130 implementation:

1. Production Elligator2 randomizes the two free high bits but still fixes the deployed-reference alternative inverse-map branch.
2. The Plan 130 integrated replay trajectory proves tunnel replay rejection and Streaming dedup but does not independently prove consumed NSR/ES tag replay at the ECIES/session layer.
3. Inbound listener selection is wire-derived, but established outbound data/control APIs and the inbound SYN-response branch do not yet make the stored I2P port tuple fully authoritative.
4. `send_data()` allocates send-window/sequence state before rejecting an oversized payload, so a locally rejected write may mutate protocol state.

## Current classification

```text
plan_126 = corrective-reopened-by-plan131-elligator-production-branch
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = superseded-by-plan130-final-gate
plan_130 = corrective-reopened-by-plan131
plan_131 = ready-for-execution
milestone6_local_product = not-closed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = hold-at-m6-local-closure
next = Plan 131
```

## Scope guard

Do not reopen NTCP2/SSU2 activation, Plan 116/117 host work, Docker/rootless namespaces, VMs, Multipass/QEMU, public-I2P testing, Python interoperability harnesses, or generalized external-router validation. Preserve the successful Plan 130 sequence/ACK/NACK, wire-derived listener, persistent duplicate-window, and full destination-stack composition work.

## Success transition

Only after Plan 131 implementation and validation satisfy every explicit acceptance criterion may current authority become:

```text
plan_130 = superseded-by-plan131-final-local-correctness-gate
plan_131 = passed-milestone6-final-local-correctness-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```
