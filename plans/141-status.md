# Plan 141 status — Milestone 7 SAM corrective authority

Status: **`active-m7-sam31-corrective-roadmap`**.

Registered: **2026-08-31**.

Source audit: Plan 140, `blocked-independent-client-stream-path-not-ready`.

Plan of record:
[`plans/141-m7-sam31-corrective-roadmap.md`](141-m7-sam31-corrective-roadmap.md).

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_135 = superseded-by-plan140-audit
plan_136 = protocol-foundation-landed-but-encoding-evidence-superseded-by-plan142
plan_137 = passed-m7-sam31-loopback-server-session-lifecycle
plan_138 = implementation-landed-but-product-acceptance-superseded-by-plan143
plan_139 = local-forward-naming-implementation-landed; final-byte-path-acceptance-deferred-to-plan144
plan_140 = blocked-audit-superseded-by-plan141-corrective-roadmap
plan_141 = active-m7-sam31-corrective-roadmap
plan_142 = passed-m7-sam31-encoding-private-destination-corrective
plan_143 = blocked-on-plan142
plan_144 = blocked-on-plan143

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately

milestone7_local_product = partially-closed (plan142-only)
sam31_stream = not-yet-product-validated
sam_independent_clients = 0-passed
router_construction = may-continue-within-m7
next_executable_plan = 143
next_product_layer = remain-on-milestone7
```

## Corrective findings now treated as authority

### SAM Base64

**Closed by Plan 142.** The i2pr SAM codec now uses the I2P Base64
alphabet (`A-Z a-z 0-9 - ~`, `=` padding) — the spelling every Java
I2P / i2pd / independent Python SAM client reference implementation
emits. The earlier RFC 4648 `+/` implementation was a protocol
defect; Plan 142 corrected it and locked the new alphabet with three
independent reference vectors (i2pd `libi2pd/Base.{h,cpp}`,
Java I2P `PrivateKeyFile.java`, i2plib `I2P_B64_CHARS = "-~"`).
See [`plans/142-status.md`](142-status.md) for the closure record.

### Private destination

**Closed by Plan 142.** The 455-byte type-7/type-4 private-destination
representation now has independent reference vectors in
`crates/i2pr-api/tests/`. The earlier evidence was circular (the i2pr
codec was its own round-trip oracle); Plan 142 replaces that with
golden vectors derived from the three independent references above.
See [`plans/142-status.md`](142-status.md) for the closure record.

### STREAM bridge

Plan 138's historical passed status does not satisfy its original product acceptance criteria. Same-socket raw CONNECT/ACCEPT, live TCP<->Streaming byte flow, retransmit/ACK driving, bounded backpressure, and full Plan-129 destination-product delivery are mandatory Plan 143 work.

### FORWARD/naming

Plan 139's ownership, loopback restriction, naming, and resource work remains useful. Its byte-path acceptance is conditional on Plan 143 and must be re-run in Plan 144.

## Execution sequence

1. [`plans/142-m7-sam31-encoding-private-destination-corrective.md`](142-m7-sam31-encoding-private-destination-corrective.md) —
   **closed as `passed-m7-sam31-encoding-private-destination-corrective`**
   per [`plans/142-status.md`](142-status.md).
2. [`plans/143-m7-sam31-live-stream-product-bridge-corrective.md`](143-m7-sam31-live-stream-product-bridge-corrective.md) —
   **next executable.**
3. [`plans/144-m7-sam31-independent-client-final-closure-corrective.md`](144-m7-sam31-independent-client-final-closure-corrective.md) —
   blocked on Plan 143.

Execute sequentially. Do not pull Plan 143 raw-stream work into Plan 142, and do not use independent-client inability to compensate for an incomplete Plan 143 product path.

## Environment policy

The corrective path remains localhost-only and compatible with the current constrained environment. It requires no root, namespaces, Docker, VM, public I2P network, or mixed-router NTCP2/SSU2 activation.

The Plan 129 authenticated-router-link-bypassed local seam remains acceptable below the destination/tunnel product stack for M7 tests. Direct Streaming-manager transfer or capture-only evidence is not acceptable.

## Handoff instruction

The next implementation model should read Plan 141 and execute **Plan 143**.

Plan 142 must create `plans/142-status.md` only after all of its acceptance criteria pass. Its status should then make Plan 143 executable. If the private-destination format cannot be independently reconciled, stop at that concrete finding rather than continuing into the raw STREAM bridge.