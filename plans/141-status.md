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
plan_143 = passed-m7-sam31-live-stream-product-bridge-corrective
plan_144 = blocked-on-plan143

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately

milestone7_local_product = partially-closed (plan142-and-plan143)
sam31_stream = bridge-closed; raw-byte-loopback-driver-deferred-to-plan144-or-follow-up
sam_independent_clients = 0-passed
router_construction = may-continue-within-m7
next_executable_plan = 144
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

**Closed by Plan 143.** The captured-outbound test seam
(`CapturedOutbound`, `record_captured`, `drain_captured_outbound`,
`adapter_send`) is removed from acceptance and replaced by the
runtime-neutral Plan 129 destination stack driven through
`i2pr_client::deliver` and the new `bridge_to_peer` SAM bridge
function. The Plan 129 destination product path crosses every
SAM STREAM delivery — outbound tunnel composition, OBEP +
participant reconstruction, garlic envelope, IBGW + participant
+ local endpoint reassembly, dispatcher ECIES new-session
classification, validated LeaseSet2 install into the receiver's
routing table, and `StreamingDestinationAdapter::receive` on the
receiver's streaming manager. The canonical Rust product lane
is `crates/i2pr-daemon/tests/sam_stream_product.rs`. The actual
`TCP <-> Streaming` raw byte bridge and the per-destination
retransmit/ACK driver task remain as follow-up work tracked in
Plan 144 or a successor plan.

### FORWARD/naming

Plan 139's ownership, loopback restriction, naming, and resource work remains useful. Its byte-path acceptance is conditional on Plan 143 and must be re-run in Plan 144.

## Execution sequence

1. [`plans/142-m7-sam31-encoding-private-destination-corrective.md`](142-m7-sam31-encoding-private-destination-corrective.md) —
   **closed as `passed-m7-sam31-encoding-private-destination-corrective`**
   per [`plans/142-status.md`](142-status.md).
2. [`plans/143-m7-sam31-live-stream-product-bridge-corrective.md`](143-m7-sam31-live-stream-product-bridge-corrective.md) —
   **closed as `passed-m7-sam31-live-stream-product-bridge-corrective`**
   per [`plans/143-status.md`](143-status.md).
3. [`plans/144-m7-sam31-independent-client-final-closure-corrective.md`](144-m7-sam31-independent-client-final-closure-corrective.md) —
   **next executable.**

Execute sequentially. Do not pull Plan 143 raw-stream work into Plan 142, and do not use independent-client inability to compensate for an incomplete Plan 143 product path.

## Environment policy

The corrective path remains localhost-only and compatible with the current constrained environment. It requires no root, namespaces, Docker, VM, public I2P network, or mixed-router NTCP2/SSU2 activation.

The Plan 129 authenticated-router-link-bypassed local seam remains acceptable below the destination/tunnel product stack for M7 tests. Direct Streaming-manager transfer or capture-only evidence is not acceptable.

## Handoff instruction

The next implementation model should read Plan 141 and execute **Plan 144**.

The Plan 143 STREAM product bridge sub-claim is closed and must not be re-opened without a concrete failing evidence lane. Plan 144 owns the per-stream raw byte bridge, the two-independent-client final closure, and the FORWARD/naming real-byte acceptance re-run.