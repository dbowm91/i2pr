# Plan 474 status amendment — inbound build reply correlation

This amendment adds evidence from four Plan 474 causal replays after the atomic owner-generation change. It does not rewrite the initial [474 status](474-status.md), change the Plan 471 historical record, or promote the SAM product claim.

**Implementation commit:** `d1a92a5` (`fix(plan474): preserve complete owner generation and trace build expiry`).

## New source evidence

The Plan 474 source now logs a count-only timeout receipt when a destination inbound build expires, including coordinator `inbound_routed`, `inbound_orphans`, peer-only correlation matches, message-only correlation matches, and pending build count. The driver retains only those bounded counts. An additional deterministic regression now verifies a real signed two-lease/receive-ID material publishes a complete **2/2** owner summary. Mismatched replacement material is rejected before installation and leaves a prior complete generation intact.

The exact-pinned `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` was rerun twice after diagnostic instrumentation:

| Run | Attempts | Matched/installed inbound build | Timeout receipt | Stock aggregate |
| --- | ---: | ---: | --- | --- |
| A | 2 | 1 / 1 | `inbound_routed=1`, `inbound_orphans=24`, `pending_builds=1` | 0 short-request decrypt failures, 0 local next-hop rejections, 0 reply-send failures; 122 aggregate transit inventory delta |
| B | 2 | 1 / 1 | `inbound_routed=1`, `inbound_orphans=38`, `inbound_orphans_peer_match=14`, `inbound_orphans_message_match=0`, `pending_builds=1` | 0 short-request decrypt failures, 0 local next-hop rejections, 0 reply-send failures; 131 aggregate transit inventory delta |
| C | 1 | 0 / 0 | `inbound_routed=0`, `inbound_orphans=30`, `inbound_orphans_peer_match=17`, `inbound_orphans_message_match=0`, `pending_builds=1` | 0 short-request decrypt failures, 0 local next-hop rejections, 0 reply-send failures; 92 aggregate transit inventory delta |
| D | 1 | 0 / 0 | `inbound_routed=0`, `inbound_orphans=16`, no selected-terminal peer matches, no message-ID offset matches, no TunnelGateway replies, `pending_builds=1` | 0 short-request decrypt failures, 0 local next-hop rejections, 0 reply-send failures; 44 transit tunnel creation events and 140 aggregate inventory delta across 4 peers |

Pinned i2pd 2.61.0 source (`TransitTunnel.cpp`, at the exact Plan 449 pin) constructs an inbound endpoint response as I2NP `ShortTunnelBuildReply` and copies `SHORT_REQUEST_RECORD_SEND_MSG_ID_OFFSET` into its message ID. For a non-endpoint short request, it forwards type 25 with that same ID to the record's next router. This supports i2pr's strict `(authenticated peer, message ID)` correlation; peer-only matches are unrelated short-build messages. The added count-only receipt also tracks TunnelGateway arrivals and same-terminal-peer ID deltas. Run D saw neither a selected-terminal-peer ShortTunnelBuild nor an inbound TunnelGateway, so its zero ID-offset buckets do not indicate an ID mismatch: the selected terminal's forwarded message was not observed at the authenticated i2pr ingress. The aggregate stock counters cannot attribute transit creation to a specific attempt, so they do not prove that the corresponding stock endpoint accepted or forwarded that attempt.

## Updated disposition

The source correction now has a 2/2 deterministic manager/bridge/owner regression and proves that a mismatched replacement cannot destroy the old installed generation. The normal stock SAM reruns still fail before the manager owner-summary gate: one run installed 1/2 inbound builds and the other installed 0/1. Consequently the earlier 0/2 owner discrepancy, stable N/N external owners, and bidirectional STREAM remain unqualified.

The first missing external transition remains the authenticated stock reply reaching i2pr ingress. Current stock evidence does not identify whether each timed-out request was accepted, forwarded, returned by SSU2, or lost before i2pr ingress. Do not retry this unchanged topology again. The next executable work must add an attempt-specific source-owned boundary receipt without retaining raw IDs or reference logs, or establish a concrete fixture/source defect and regression before replay.

**Status token remains:** `blocked-stock-sam-attempt-stops-before-owner-summary-and-stream`.  
**Unresolved finding:** high — normal SAM inbound build and bidirectional application path remain unqualified.  
**No dependent plan is unblocked.**
