# Plan 221 status — M6 Java client-NetDB/OCMOSJ narrowing

Status: **`superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective`**.

Plan of record:
[`221-m6-java-client-netdb-ocmosj-narrowing.md`](../../implementation/mixed-router-interop/221-m6-java-client-netdb-ocmosj-narrowing.md).

## Disposition

Plan 221 was registered but not executed.

Exact-pinned Java I2P 2.13.0 source review found that two assumptions inherited
from Plan 220 were too weak for a trustworthy client-message attribution:

1. the selector probe used the raw Destination hash and a hard-coded N=3,
   whereas `IterativeSearchJob` derives a daily routing key with
   `routingKeyGenerator().getRoutingKey(key)` and selects
   `_totalSearchLimit + EXTRA_PEERS`; and
2. the public listener-enabled
   `I2PSession.sendMessage(..., SendMessageStatusListener)` API provides a
   stronger nonce-correlated observation path than Plan 221's generic
   log/stat-first ladder.

Plan 222 supersedes this plan before execution. No Plan 221 implementation or
external result is authoritative.

## Retained facts

Plan 220 still establishes:

- the RouterHash cross-check;
- Router A stores Router B's exact current `f`-bearing RouterInfo;
- Router A PeerManager indexes Router B under `f`;
- Java admits the reverse client send;
- no reverse TunnelData/payload reaches i2pr inside the existing 45-second
  acceptance window; and
- the old J219-B bootstrap hypothesis is refuted.

The Plan 220 selector-pass row is historical evidence only until Plan 222
re-runs the selector with production-equivalent inputs.

## Handoff

```text
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective
next_executable_plan = 222-m6-java-client-netdb-ocmosj-narrowing-corrective
```
