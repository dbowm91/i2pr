# Plan 123 status — final local closure (restored by Plans 128/129)

## Current authority

- Status: **`passed-corrected-streaming-wire-local`**.
- Final closure: **2026-08-25**. Plan 128 corrected the packet wire
  format; Plan 129 closed the integrated Milestone 6 local-product
  gate and restored this status as the final Streaming classification.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.
- Wire corrective plan: `128-m6-streaming-wire-protocol-corrective-closure.md`.
- Integrated gate: `plans/129-status.md`.

## Delivered surface

- 22-byte fixed packet header with the normative Plan 128 flag map
  (`INITIAL_SYN_FLAGS 0x04A9`, `SYN_RESPONSE_FLAGS 0x00A9`,
  `CLOSE_FLAGS 0x000A`, `RESET_FLAGS 0x000C`);
- connection/listener tables and resource ceilings;
- sequence/ACK/NACK, retransmission (now with the Plan 129 integrated
  `poll_retransmits` path), congestion, reordering;
- signed control packets over the canonical zeroed-placeholder
  preimage; CLOSE/RESET verify against the retained peer identity;
- real `OutboundSynSent` -> peer SYN-response -> `Established`
  progression with min-of-advertisements negotiation;
- protocol-6 RFC 1952 gzip client-payload framing from Plan 125;
- corrected monotonic `SystemClock`.

The direct VirtualWire tests remain fast unit coverage only; they are
not closure evidence. The authoritative evidence is the Plan 129
integrated trajectory suite
(`crates/i2pr-client/tests/plan129_trajectory.rs`) over the complete
destination stack.

## Current classification

```text
plan_123 = passed-corrected-streaming-wire-local
plan_125 = superseded-by-final-corrective-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (Milestone 7)
```

Do not claim mixed-router Streaming interoperability.
