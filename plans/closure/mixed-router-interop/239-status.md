# Plan 239 status — M6 Java Streaming Router-A pre-dispatch / OCMOSJ attribution

Status: **registered-ready-m6-java-streaming-router-a-dispatch-observer**.

Plan of record:
[`239-m6-java-streaming-router-a-dispatch-observer.md`](../../implementation/mixed-router-interop/239-m6-java-streaming-router-a-dispatch-observer.md).

## Registration basis

Plan 238 closed as:

```text
passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
```

on implementation SHA:

```text
08df6ead671cc0201bf77205056211cfdcb8df15
```

Three counted attempts proved:

```text
helper sendMessage delta = 3
Router-A client.distributeTime delta = 3
Router-A client.dispatchTime delta = 0
Router-A client.dispatchSendTime delta = 0
```

The historical terminal label `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` is
retained for closure traceability, but the executed Router-A evidence proves
I2CP admission occurred. Plan 239 therefore owns the narrower **post-admission,
pre-dispatch OCMOSJ** interval.

## Exact-pinned source correction

Exact Java I2P 2.13.0 source at
`9134f808337b401e8e53c73734c81fab04280c9d` shows:

- OCMOSJ constructor first performs client-NetDB
  `lookupLeaseSetLocally(toHash)`;
- `client.leaseSetFoundRemoteTime` is incremented only when a remote lookup
  had been required and later succeeds;
- `client.leaseSetFailedRemoteTime` records remote lookup failure;
- `client.dispatchNoTunnels` covers at least two distinct branches:
  outbound-tunnel selection failure and later garlic/tunnel-material failure;
- `DispatchJob.runJob()` calls
  `tunnelDispatcher().dispatchOutbound(...)` before
  `client.dispatchTime` / `client.dispatchSendTime`;
- `client.dispatchPrepareTime` is added only after the inline DispatchJob
  returns to `send()`.

Therefore Plan 239 must not infer “no target LeaseSet” from a zero remote
lookup-success counter and must not infer which tunnel branch failed from
`client.dispatchNoTunnels` alone.

## Current authority

```text
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
plan_239 = registered-ready-m6-java-streaming-router-a-dispatch-observer

plan_201 = blocked-after-plan238-client-message-not-admitted-pending-router-a-dispatch-observer-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan239
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 239-m6-java-streaming-router-a-dispatch-observer

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

No production change is authorized by this registration.
