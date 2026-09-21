# Plan 231 status — M6 Java reverse-delivery tunnel-dispatch attribution corrective

Status: **registered-ready-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective**.

Plan of record:
[`231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective.md`](../../implementation/mixed-router-interop/231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective.md).

## Registration basis

Plan 230 closed as:

```text
passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
```

Its deepest run proved the corrected Java profile/bootstrap path, genuine
non-zero exploratory tunnels, one-hop client tunnels through C, target LS2
resolution, and digest-matched i2pr -> Java raw Destination delivery. The
tracked Java -> i2pr reverse send produced `STATUS_SEND_ACCEPTED` but no
matching payload in the frozen 45-second window and no later terminal status.

Exact-pinned Java I2P 2.13.0 source review further narrows that boundary:
`ClientMessageEventListener.handleSendMessage()` calls
`distributeMessage()`, the client message pool runs OCMOSJ inline, OCMOSJ runs
its `DispatchJob` inline, and `DispatchJob` calls
`TunnelDispatcher.dispatchOutbound(...)` before returning. Only after
`distributeMessage()` returns does `ackSendMessage()` emit
`STATUS_SEND_ACCEPTED`.

Therefore Plan 231 does not investigate whether OCMOSJ was entered. It
attributes the post-dispatch path:

```text
Java A outbound gateway enqueue
 -> Java C one-hop outbound endpoint
 -> selected target lease gateway / inbound gateway
 -> emitted TunnelData
 -> i2pr exact inbound tunnel
 -> tunnel recovery
 -> Garlic decode
 -> Destination payload
```

## Current authority

```text
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = registered-ready-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective

plan_201 = blocked-pending-plan231-reverse-delivery-tunnel-dispatch-attribution-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan231
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## Authorized work

Plan 231 may add read-only test/harness observability sufficient to classify:

- exact Java A outbound gateway enqueue/drop;
- exact Router-C outbound-endpoint processing for the installed one-hop client
  tunnel;
- exact target lease gateway / inbound-gateway processing;
- next-hop TunnelData emission toward i2pr;
- i2pr exact target TunnelData receipt;
- tunnel recovery, Garlic decoding, Destination dispatch, and payload digest.

It may not tune the Plan-230 fixture or change production behavior merely to
make the lane pass.

## Prohibited shortcuts

No Java patching/reflection/private mutation; no direct queue/tunnel/NetDB/profile
injection; no timeout inflation; no topology/profile/tunnel-policy changes; no
public I2P/reseed/VMComm; no `netDb.alwaysQuery`; no global counter used as the
sole target-message proof; no unrelated TunnelData accepted as the target; raw
logs remain scratch-only.

Closure must replace this registration token with exactly one earliest-stage
Plan-231 terminal or a digest-matched reverse-delivery pass, then update the
Plan-201/204 unblock audit.
