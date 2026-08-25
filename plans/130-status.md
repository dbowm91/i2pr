# Plan 130 status — Milestone 6 final wire/runtime corrective closure

## Current authority

Status: **`ready-for-execution`**.

- Registered: **2026-08-25**.
- Source floor: `29fb88d36f9794202e88d4b947faed30569c1991`.
- Plan of record: `plans/130-m6-final-wire-runtime-corrective-closure.md`.
- Roadmap authority: `plans/126-130-milestone6-final-corrective-roadmap.md`.

This status file and the 126–130 roadmap supersede all earlier current-authority statements that say `milestone6_local_product = passed` until Plan 130 executes successfully. Earlier Plan 126–129 status files remain historical implementation/evidence records.

## Why the Plan 129 closure is reopened

The post-Plan-129 audit retained the integrated architecture but found four narrow defects:

1. **Production Elligator2 representation fingerprinting:** the current production path uses a fixed representation/tweak choice instead of the randomized on-wire behavior expected by I2P reference implementations.
2. **Streaming sequence/ACK semantics:** ordinary application data starts at sequence zero; simple ACK / `ackThrough == 0` handling and standalone delayed ACK behavior are incomplete.
3. **I2P destination-port routing:** the inbound adapter decodes `destination_port` but a separate caller-supplied listener value can determine the actual listener backlog.
4. **Replay evidence:** the Plan 129 fixture rebuilds inbound tunnel role/duplicate-window state before ordinary deliveries, so the exact tunnel-replay claim is not proven against one persistent live window.

The following Plan 129 work is still retained:

- full Streaming -> gzip -> Data -> ECIES -> Garlic -> destination tunnel -> OBEP -> seam -> inbound tunnel -> dispatcher -> ECIES -> Data -> gzip -> Streaming topology;
- Plan 127 sender-LS2 binding and reverse NSR/Existing-Session routing;
- Plan 128 corrected flags/options/signatures/MTU/SYN policy;
- runtime-neutral adapter architecture;
- existing corruption, loss/retransmission, reorder, CLOSE, RESET, and resource-bound tests, subject to corrected sequence/ACK semantics.

## Current classification

```text
plan_126 = corrective-reopened-plan130-elligator-wire-randomization
plan_127 = passed-destination-session-routing-final-closure
plan_128 = corrective-reopened-plan130-streaming-sequence-ack
plan_129 = corrective-reopened-plan130
plan_130 = ready-for-execution
milestone6_local_product = not-closed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
next = plans/130-m6-final-wire-runtime-corrective-closure.md
```

## Execution constraint

Plan 130 is intentionally a **single narrow pass**. Do not split it into a new validation program unless implementation uncovers a concrete independent protocol defect that cannot safely be corrected inside the Plan 130 surfaces.

Do not reopen NTCP2/SSU2 activation, public I2P, external router lanes, Docker/rootless namespaces/VMs, Python harnesses, SAM, I2CP sockets, proxies, or service tunnels as part of this closure.

## Required final classification after successful execution

```text
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = superseded-by-plan130-final-gate
plan_130 = passed-milestone6-final-wire-runtime-corrective-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

Until those Plan 130 acceptance criteria are actually green, SAM implementation should not begin.