# Plan 132 status — Milestone 6 final evidence and transactional closure

Status: **`ready-for-execution`**.

- Registered: **2026-08-27**.
- Source floor: `beb24fd945db3bcbc753c22c112cf3556a04dca6`.
- Plan of record: [`plans/132-m6-final-evidence-and-transactional-closure.md`](132-m6-final-evidence-and-transactional-closure.md).
- This file is the **current Milestone 6 authority** and supersedes the closure interpretation in `plans/131-status.md` until Plan 132 passes.
- `plans/131-status.md` remains historical evidence for the Plan 131 implementation commit. Its `milestone6_local_product = passed` and “next Milestone 7” statements are reopened by this status file because the post-implementation audit found invalid replay-layer evidence, over-permissive Elligator representative decoding, and mutation-before-fallible-wire-construction in established send APIs.

## Current classification

```text
plan_130 = superseded-by-plan131
plan_131 = corrective-reopened-by-plan132
plan_132 = ready-for-execution

milestone6_local_product = not-closed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = hold-for-plan132-local-closure
next = Plan 132
```

## Narrow corrective surface

Plan 132 contains exactly three closure areas:

1. **Strict Elligator receive-domain validation**
   - retain Plan 131 randomized production encoding;
   - mask the two free high bits;
   - reject non-canonical representatives before the reviewed inverse map;
   - preserve valid Java I2P/i2pd branch/high-bit fixtures and frozen Plan 126 ECIES vectors.

2. **Real three-layer replay evidence**
   - replay the exact same retained tunnel cell and assert typed tunnel duplicate rejection;
   - replay the exact same consumed ES ciphertext/session tag in freshly generated tunnel cells and assert ECIES/session rejection before plaintext;
   - freshly ECIES-seal the exact same Streaming sequence under a new valid session tag, require tunnel+ECIES success, and assert Streaming-only duplicate suppression.

3. **Transactional established sends**
   - perform all fallible `send_data()` packet/envelope construction before send-window commit;
   - construct/sign CLOSE before close-state mutation;
   - construct/sign RESET before reset-state mutation;
   - retain connection-owned I2P ports, source-port-zero behavior, corrected sequence/ACK semantics, and existing successful wire output.

## Explicitly unchanged

- No mixed-router interoperability is claimed.
- External transport acceptance debt remains separate.
- NTCP2 remains experimental/non-advertised.
- No Plan 116/117, Emissary, rootless, Docker, VM, Multipass, QEMU, public-network, Python interop-harness, SAM, I2CP, proxy, or service-tunnel work is part of Plan 132.

## Required closure state

Only after the executable Plan 132 acceptance criteria pass should repository authority become:

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-final-evidence-and-transactional-gate
plan_132 = passed-milestone6-final-evidence-and-transactional-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

At that point Milestone 6 corrective planning stops and Milestone 7 / SAM baseline planning is authorized.
