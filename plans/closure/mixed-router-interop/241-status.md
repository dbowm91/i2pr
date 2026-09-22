# Plan 241 status — M6 Java Streaming one-hop client-tunnel fixture corrective

Status: **registered-ready-m6-java-streaming-one-hop-client-tunnel-fixture-corrective**.

Plan of record: [`241-m6-java-streaming-one-hop-client-tunnel-fixture-corrective.md`](../../implementation/mixed-router-interop/241-m6-java-streaming-one-hop-client-tunnel-fixture-corrective.md).

## Registration basis
Plan 240 closed as `passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary` on implementation SHA `37025f9eacfbe8defaae873f77ea2923249be912`. Three counted Streaming runs proved Router B eligible and in the exact target ISJ `toTry`, then proved `sendQuery()` rejected B because the selected outbound client tunnel was zero-hop and the helper client facade did not contain B RI. The exact terminal was `P240-C-B-ZERO-HOP-UNKNOWN-RI` on all three runs.

## Corrective authorization
The current `ReferenceStreamingService` hard-codes `inbound.length=0`, `outbound.length=0`, and `allowZeroHop=true` in both directions. Exact-pinned Java selects from the destination-specific client pool and applies the client-facade unknown-RI guard only when the selected outbound tunnel length is <= 1. The repo already carries a proven one-hop SessionConfig pattern in `ReferenceRawDestination`, and Plan 230 demonstrated genuine one-hop client tunnels through Router C with zero-hop absent when the controlled bootstrap succeeds. Plan 241 therefore owns only the Streaming helper fixture correction plus ordered continuation.

## Current authority
```text
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
plan_241 = registered-ready-m6-java-streaming-one-hop-client-tunnel-fixture-corrective

plan_201 = blocked-after-plan240-b-zero-hop-unknown-ri-pending-plan241-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan241
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 241-m6-java-streaming-one-hop-client-tunnel-fixture-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

No production i2pr change, Java-router patch, RI injection, topology change, profile mutation, publication workaround, or timing change is authorized by this registration.