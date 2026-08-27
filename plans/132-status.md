# Plan 132 status — Milestone 6 final evidence and transactional closure

Status: **`implementation-landed-evidence-superseded-by-plan133`**
(historical implementation/evidence record; final Milestone 6 local
closure authority is
`passed-milestone6-final-evidence-authority-closure` per
[`plans/133-status.md`](133-status.md)).

- Registered: **2026-08-27**.
- Source floor: `beb24fd945db3bcbc753c22c112cf3556a04dca6`.
- Plan of record: [`plans/132-m6-final-evidence-and-transactional-closure.md`](132-m6-final-evidence-and-transactional-closure.md).
- Plan 132's substantive implementation is retained unchanged:
  strict Elligator receive validation in
  `crates/i2pr-crypto/src/ecies.rs`; Plan 131 production Elligator
  branch/high-bit randomization; transactional `send_data()` wire
  construction before send-window commit; transactional CLOSE/RESET
  build/sign before state mutation; exact tunnel-cell replay B1
  evidence; connection-owned I2P ports and source-port-zero
  behavior; corrected Streaming sequence/ACK/NACK behavior; Plan
  124 encrypted Garlic/tunnel composition invariant.
- Plan 132 reopened the Plan 131 closure because the
  post-implementation audit found three narrow gaps: an
  over-permissive Elligator representative decoder (recipients
  accepted representatives in the upper half of the field);
  invalid replay-layer evidence (B2/B3 compared freshly
  synthesized envelopes rather than retaining the real first
  delivered ES envelope); and mutation-before-fallible-wire-
  construction in `send_data()` / `send_close()` / `send_reset()`.
  Plan 132 corrected the decoder and the transactional send APIs
  and rewrote B1 as the exact tunnel-cell replay. Plan 133 then
  corrected B2 and B3 to retain the real first ES envelope and
  reconciled the Elligator reference note's equality-boundary
  wording. This file is now a historical record; Plan 133 is the
  authoritative final Milestone 6 local closure.

## Final classification

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-and-plan133-final-gates
plan_132 = implementation-landed-evidence-superseded-by-plan133
plan_133 = passed-milestone6-final-evidence-authority-closure (current authority)

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

## Narrow corrective surface (Plan 132 implementation, retained)

Plan 132 closed exactly three closure areas:

1. **Strict Elligator receive-domain validation**
   - retain Plan 131 randomized production encoding;
   - mask the two free high bits;
   - reject non-canonical representatives before the reviewed inverse map;
   - preserve valid Java I2P / i2pd branch/high-bit fixtures and frozen Plan 126 ECIES vectors.

2. **Real three-layer replay evidence (B1 only — B2/B3 are corrected by Plan 133)**
   - replay the exact same retained tunnel cell and assert typed tunnel duplicate rejection;
   - freshly ECIES-seal the exact same Streaming sequence under a new valid session tag, require tunnel + ECIES success, and assert Streaming-only duplicate suppression;
   - the B2 consumed ES replay was later rewritten in Plan 133 to retain the actual first delivered ES envelope rather than freshly synthesizing a comparison artifact.

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

## Handoff

Per the plan-of-record §13: stop corrective Milestone 6 planning.
The next product work is **Milestone 7 / SAM baseline planning**.
Do not reopen external transport validation as a prerequisite for
SAM.
