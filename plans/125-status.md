# Plan 125 status — superseded by the final corrective closure

## Current authority

- Status: **`superseded-by-final-corrective-closure`** (Plan 129,
  2026-08-25).
- Original Plan 125 commit: `523d5dcd87f6c04853a016f7b54e3922697ffb2b`.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.
- Integrated gate closure record: [`plans/129-status.md`](129-status.md).

## Retained Plan 125 corrections

These changes are preserved in the product and remain load-bearing:

- RFC 1952 I2P client-payload gzip framing with source/destination
  ports and protocol byte;
- removal of the old custom zlib + SHA-256 envelope;
- originator remains `OutboundSynSent` until a peer SYN response is
  processed;
- explicit responder SYN-response construction;
- stream-id ownership split between local receive id and peer receive
  id;
- `SystemClock` anchored at a fixed `Instant` origin.

## Why Plan 125 no longer owns the Milestone 6 gate

The post-Plan-125 audit found that the destination ECIES ratchet, the
Streaming packet wire format, and several adapter/state-machine
details required the Plans 126–128 corrective passes before any
`milestone6_local_product = passed` claim could be honest. Plan 129
completed the integrated two-direction gate over the corrected stack
(including the inbound adapter and correct adapter sizing Plan 125
lacked) and now owns the final classification. Plan 125's earlier
"local-product gate" claim is superseded, not deleted; its retained
fixes are listed above.

## Current classification

```text
plan_123 = passed-corrected-streaming-wire-local
plan_125 = superseded-by-final-corrective-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (Milestone 7)
```
