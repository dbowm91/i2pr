# Plan 236 status — M6 Java Streaming response-emission and I2CP send attribution

Status: **registered-ready-m6-java-streaming-response-emission-and-i2cp-send-attribution**.

Plan of record:
[`236-m6-java-streaming-response-emission-and-i2cp-send-attribution.md`](../../implementation/mixed-router-interop/236-m6-java-streaming-response-emission-and-i2cp-send-attribution.md).

## Registration basis

Plan 235 closed as:

```text
passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
```

on implementation SHA:

```text
61d75ae60b41392633512ff5f59b1318965695f9
```

with three same-SHA counted attempts producing:

```text
P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND
```

The stable boundary is:

```text
stock Java Streaming accepted the inbound SYN
-> I2PServerSocket.accept() returned
-> returned I2PSocket input/output surfaces were usable
-> i2pr outbound Streaming request was accepted
-> i2pr outbound cell dispatch accepted 2/2
-> no inbound TunnelData reached i2pr
```

That evidence does **not** prove that Java constructed, queued, or submitted
the response packet.

Plan 236 therefore owns only the exact missing interval:

```text
Java accepted inbound Streaming connection
-> Java response scheduler / packet construction
-> Connection.sendPacket
-> PacketQueue.enqueue
-> I2PSession.sendMessage
-> Java Router A client-message admission/dispatch
-> route-derived target IBGW
-> i2pr exact TunnelData
```

## Current authority

```text
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
plan_236 = registered-ready-m6-java-streaming-response-emission-and-i2cp-send-attribution

plan_201 = blocked-pending-plan236-java-streaming-response-emission-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan236
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 236-m6-java-streaming-response-emission-and-i2cp-send-attribution

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## Verification caveat carried into Plan 236

The prior workspace-wide serial test run was stopped after hanging in
`sam_stream_final_acceptance`; it remains **incomplete**, not passing.

Plan 236 treats that as a separate exact-head verification-hygiene issue. It
must not be used as evidence for or against the Java Streaming response path.
If it recurs, Plan 236 records the exact hanging SAM test and may close a
diagnostic Java boundary, but Java-family/M6 final closure remains forbidden
until the workspace floor completes successfully or a separately registered
SAM test-hygiene corrective closes the issue.

No production change is authorized by this registration.
