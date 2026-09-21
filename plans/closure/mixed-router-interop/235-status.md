# Plan 235 status — M6 Java Streaming post-accept response boundary corrective

Status: **registered-ready-m6-java-streaming-post-accept-response-boundary-corrective**.

Plan of record:
[`235-m6-java-streaming-post-accept-response-boundary-corrective.md`](../../implementation/mixed-router-interop/235-m6-java-streaming-post-accept-response-boundary-corrective.md).

## Registration basis

Plan 234 closed with the exact terminal
`P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED`: Java's bounded accept state was
`requested=1 entered=1 returned=1 stored=1 errors=0`, while i2pr observed no
inbound `TunnelData` in the frozen SYN epoch. The retained Plan-232 route
parity and raw-Destination reverse pass remain valid. Plan 235 is a narrow
follow-up at that post-accept / pre-i2pr-inbound boundary.

```text
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = registered-ready-m6-java-streaming-post-accept-response-boundary-corrective
plan_201 = blocked-pending-plan235-java-streaming-post-accept-response-boundary-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan235
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = 235-m6-java-streaming-post-accept-response-boundary-corrective
```

Plan 235 must not change production Rust behavior without a separately proven
i2pr-owned defect and a new production corrective plan.
