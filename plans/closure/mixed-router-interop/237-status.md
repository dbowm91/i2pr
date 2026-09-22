# Plan 237 status — M6 Java Streaming stock-response observability corrective

Status: **registered-ready-m6-java-streaming-stock-response-observability-corrective**.

Plan of record:
[`237-m6-java-streaming-stock-response-observability-corrective.md`](../../implementation/mixed-router-interop/237-m6-java-streaming-stock-response-observability-corrective.md).

## Registration basis

Plan 236 closed as:

```text
passed-m6-java-streaming-response-emission-observability-gap
```

on implementation SHA:

```text
41b6ccfea16ca051f31605dbf89bf749d0cc77e5
```

The exact Java I2P 2.13.0 response path was source-locked, but the helper still emitted literal false placeholders for every response stage. Two same-SHA counted attempts therefore stopped honestly at:

```text
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
```

Plan 237 corrects only that missing observer.

## Exact source-derived observation opportunities

The pinned Java source already provides bounded stock signals:

- `SchedulerReceived.eventOccurred()` DEBUG:
  `received con... send a packet` / `received con... time till next send`;
- `Connection.ackImmediately()` DEBUG:
  `sending new ack:`;
- `PacketQueue.enqueue()` calls `I2PSession.sendMessage(...)` and then increments
  `stream.con.sendMessageSize`;
- `ConnectionManager` creates that `RateStat`;
- public `RateStat.getLifetimeEventCount()` provides a bounded count.

Plan 237 should use pre/post isolated-epoch deltas rather than adding another large probe framework.

## Current authority

```text
plan_236 = passed-m6-java-streaming-response-emission-observability-gap
plan_237 = registered-ready-m6-java-streaming-stock-response-observability-corrective

plan_201 = blocked-pending-plan237-stock-response-observability-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan237
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 237-m6-java-streaming-stock-response-observability-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## Verification caveat

The recurring full-workspace stall in `sam_stream_final_acceptance` remains a separate verification-hygiene issue. Plan 237 does not own SAM changes.

A diagnostic Plan-237 boundary may close with an incomplete workspace floor if production code remains unchanged, but final Java-family/M6 closure remains forbidden until that floor is genuinely green or separately corrected.
