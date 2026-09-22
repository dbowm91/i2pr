# Plan 240 status — M6 Java Streaming target-LeaseSet lookup-failure attribution

Status: **registered-ready-m6-java-streaming-target-leaseset-lookup-failure-attribution**.

Plan of record:
[`240-m6-java-streaming-target-leaseset-lookup-failure-attribution.md`](../../implementation/mixed-router-interop/240-m6-java-streaming-target-leaseset-lookup-failure-attribution.md).

## Registration basis

Plan 239 closed as:

```text
passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary
```

on implementation SHA:

```text
d82cf06cfc7e1ba46aaf189317e99637e7f87dc4
```

Three counted Streaming runs proved:

```text
Router-A admission                   client.distributeTime +3
helper client-subDB target LS        absent
remote lookup success                +0
remote lookup failure                +3 / +4 / +4
dispatchTime / dispatchSendTime      +0 / +0
helper client tunnels                outbound=1 inbound=1
```

The exact stop is:

```text
P239-D-TARGET-LEASESET-LOOKUP-FAILED
```

## Retained lookup authority

Plan 225 previously proved, in the destination lane:

```text
P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B
```

with Router B's target LS2 current/query-answerable and Router A's helper
client DB empty.

Plan 226 retained exact target-job/Router-B/IP-close correlation and did not
authorize the proposed topology correction.

Plan 240 reuses those diagnostics in the Plan-239 Streaming epoch rather than
building another lookup harness.

## Exact-pinned source basis

Java I2P 2.13.0 at
`9134f808337b401e8e53c73734c81fab04280c9d` establishes the relevant order:

```text
OCMOSJ local client-DB lookup miss
-> client NetDB lookup
-> IterativeSearchJob negative-cache guard
-> FloodfillPeerSelector candidate selection
-> New ISJ ... toTry
-> retry / IP diversity
-> sendQuery pre-dispatch guards
-> exact target DLM dispatch
-> B receipt / answer
-> A client-tunnel DSM
-> helper client-subDB install
```

Plan 240 must correlate the exact Streaming lookup by helper DBID + target hash
+ ISJ job ID and stop at the first proven missing stage.

## Current authority

```text
plan_239 = passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary
plan_240 = registered-ready-m6-java-streaming-target-leaseset-lookup-failure-attribution

plan_201 = blocked-after-plan239-target-leaseset-lookup-failed-pending-plan240-attribution-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan240
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 240-m6-java-streaming-target-leaseset-lookup-failure-attribution

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

No corrective behavior is authorized by this registration. The Plan-240
terminal must determine the next plan-of-record.
