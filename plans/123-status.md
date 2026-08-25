# Plan 123 status — Streaming wire corrective closure required

## Current authority

- Status: **`corrective-reopened-plan128`**.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.
- Streaming corrective plan: `128-m6-streaming-wire-protocol-corrective-closure.md`.
- Final integration gate: Plan 129.

## Retained work

The Plan 123/125 implementation retains useful local Streaming functionality:

- 22-byte fixed packet header structure;
- connection/listener tables and resource ceilings;
- ACK/NACK/retransmission/congestion/reordering logic;
- signed control-packet concept and signature preimage helper;
- real `OutboundSynSent` -> peer SYN-response -> `Established` state progression;
- protocol-6 RFC 1952 gzip client-payload framing from Plan 125;
- corrected monotonic `SystemClock`.

## Why closure is reopened

The packet wire format remains non-standard:

```text
flag bits are misassigned
option data uses invented TLVs
MAX_PACKET_SIZE is encoded as TLV + u32 instead of raw u16
signature parser assumes synthetic TLV/fixed 64-byte shape
initial/reply NO_ACK behavior is reversed
Proposal-164 replay NACKs are incorrectly put on the SYN response
1730 is treated as a total/reduced packet budget instead of payload-only MTU
```

The direct `VirtualWire` Streaming tests remain useful fast tests, but they are not Milestone 6 closure evidence.

## Current classification

```text
plan_123 = corrective-reopened-plan128
plan_125 = corrective-reopened-plans126-129
milestone6_local_product = not-closed
next_global = plans/126-m6-ecies-destination-ratchet-corrective-foundation.md
streaming_next = plans/128-m6-streaming-wire-protocol-corrective-closure.md
```

Do not claim mixed-router Streaming interoperability.