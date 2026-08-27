# Plan 133 status — Milestone 6 final evidence and authority closure

Status: **`ready-for-execution`**.

- Registered: **2026-08-27**.
- Source floor: `af0f07a0037a639afc2c03af31a1266b99273564`.
- Plan of record: [`plans/133-m6-final-evidence-authority-closure.md`](133-m6-final-evidence-authority-closure.md).
- This file is the **current Milestone 6 local closure authority** until Plan 133 executes.
- Plan 132's substantive implementation corrections are retained, but its closure evidence/authority is incomplete: B2 does not directly assert the integrated ECIES/session rejection variant; B3 compares later fresh seals rather than retaining the actual first delivered ES envelope; the Elligator reference note incorrectly states that Java I2P and i2pd agree on the equality boundary; and Plan 131/132/current-facing status documents disagree about which plan closes Milestone 6.
- Do not reopen transport or host-validation work. This is one narrow evidence/documentation/status closure pass.

## Current classification

```text
plan_130 = superseded-by-plan131
plan_131 = corrective-history-under-plan133
plan_132 = implementation-landed-evidence-authority-incomplete
plan_133 = ready-for-execution

milestone6_local_product = not-closed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = hold-for-plan133-final-evidence-authority-closure
next = Plan 133
```

## Retained Plan 132 implementation

The following are **not** reopened absent a concrete failing regression:

- strict Elligator receive validation in `crates/i2pr-crypto/src/ecies.rs`;
- Plan 131 production Elligator branch/high-bit randomization;
- transactional `send_data()` wire construction before send-window commit;
- transactional CLOSE/RESET build/sign before state mutation;
- exact tunnel-cell replay B1 evidence;
- connection-owned I2P ports and source-port-zero behavior;
- corrected Streaming sequence/ACK/NACK behavior;
- Plan 124 encrypted Garlic/tunnel composition invariant.

## Plan 133 closure surface

Plan 133 contains exactly four tasks:

1. **B2 evidence correction**
   - retain exact first Existing Session ciphertext/tag;
   - rewrap it only below ECIES in fresh tunnel cells;
   - prove tunnel acceptance;
   - assert typed integrated `InboundDispatchOutcome::Rejected(InboundDispatchError::Session(...))` or the exact stable ECIES/session rejection variant reached by the production dispatcher;
   - retain direct `UnknownSessionTag` proof and no-plaintext negative controls.

2. **B3 evidence correction**
   - create one Streaming `TransportSendRequest` sequence `N`;
   - retain the actual first ES plan and deliver it;
   - freshly reseal that exact request once;
   - prove first/second ES tags differ while encoded Streaming bytes/sequence remain identical;
   - prove second tunnel + dispatcher/ECIES succeed;
   - prove only Streaming suppresses the duplicate sequence;
   - remove the hard-coded one-cell comparison helper.

3. **Elligator reference-note correction**
   - Java executable check: rejects equality (`r >= threshold` rejected);
   - pinned i2pd executable check: accepts equality (`BN_cmp(r, p12) <= 0` enters decode), despite its source comment saying `<`;
   - retain i2pr's stricter Java-compatible `r < threshold` rule and document it as a deliberate strict subset, not unanimous reference behavior.

4. **Final authority synchronization after green validation**
   - make Plans 131/132 historical/superseded records;
   - make Plan 133 the sole final local Milestone 6 closure authority;
   - synchronize README/AGENTS/architecture/protocol-support/support-table/skill current-facing status;
   - authorize Milestone 7 / SAM baseline planning.

## Explicitly unchanged / out of scope

- No mixed-router interoperability is claimed.
- External transport acceptance debt remains separate.
- NTCP2 remains experimental/non-advertised.
- No Plan 116/117, Emissary, rootless, Docker, VM, Multipass, QEMU, public-network, Python interop-harness, SAM implementation, I2CP, proxy, service-tunnel, new LeaseSet, PQ, or legacy ElGamal work belongs in Plan 133.

## Required closure state

Only after Plan 133's executable evidence and full local regression gate pass should repository authority become:

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-and-plan133-final-gates
plan_132 = passed-implementation-corrections-superseded-by-plan133-evidence-authority-gate
plan_133 = passed-milestone6-final-evidence-authority-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

At that point Milestone 6 corrective planning stops. Do not create another Milestone 6 plan unless a new concrete protocol defect is discovered.