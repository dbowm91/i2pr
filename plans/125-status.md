# Plan 125 status — corrective closure reopened

## Current authority

- Status: **`corrective-reopened-plans126-129`**.
- Reopened: **2026-08-25** after post-closure protocol/source audit.
- Original Plan 125 commit: `523d5dcd87f6c04853a016f7b54e3922697ffb2b`.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.

## Retained Plan 125 corrections

These changes are useful and should be preserved:

- RFC 1952 I2P client-payload gzip framing with source/destination ports and protocol byte;
- removal of the old custom zlib + SHA-256 envelope;
- originator remains `OutboundSynSent` until a peer SYN response is processed;
- explicit responder SYN-response construction exists;
- stream-id ownership is clearer than the Plan 123 source floor;
- `SystemClock` now uses a fixed `Instant` origin;
- outbound `StreamingDestinationAdapter` establishes the intended architectural direction.

## Why `milestone6_local_product = passed` is withdrawn

The post-closure audit found additional local product defects:

1. the destination ECIES ratchet below Streaming is not current I2P wire/session compatible and requires Plans 126–127;
2. Streaming flag assignments and option-data encoding remain non-standard and require Plan 128;
3. replay NACK / NO_ACK SYN-response policy is wrong;
4. the Streaming MTU is treated with the wrong payload-vs-packet semantics;
5. the outbound adapter uses the Streaming application-payload bound for a complete gzip client payload;
6. no inbound adapter completes Data -> gzip -> protocol/ports -> Streaming dispatch;
7. no authoritative test drives SYN, SYN response, data, retransmission, CLOSE, and RESET over the complete destination stack.

## Current classification

```text
plan_125 = corrective-reopened-plans126-129
milestone6_local_product = not-closed
milestone6_interoperable = not-claimed
next = plans/126-m6-ecies-destination-ratchet-corrective-foundation.md
```

Final closure authority belongs only to Plan 129.