# Plan 209 status — M10 product-only remote HTTP/IRC acceptance corrective

Status: **`registered-blocked-by-plan208`**.

Plan of record: [`209-m10-product-only-remote-http-and-irc-application-acceptance-corrective.md`](209-m10-product-only-remote-http-and-irc-application-acceptance-corrective.md).

Plan 209 supersedes the over-promoted closure interpretation of Plan 207. Plan 207 introduced valuable real application clients (system curl and exact-pinned jaraco/irc) and fixture-derived facts, but its counted driver still constructs and drives a parallel destination/tunnel/Streaming/router stack and manually advances legacy transport counters. Those facts are retained as scaffolding, not final product evidence.

Current authority:

```text
plan_207 = retained-partial-real-application-client-harness-superseded-by-plan209
plan_208 = registered-executable-m10-production-delivery-driver-remote-route-integration
plan_209 = registered-blocked-by-plan208
m10_remote_transport_core = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
```

Plan 209 may execute as a pass attempt only after Plan 208 is passed. The counted application driver must then be reduced to a black-box product harness: start the real product composition, run real curl/jaraco clients against actual manager listeners, inspect fixture facts and operation-derived Plan 208 counters, and stop the product. It must not directly construct or drive `StreamingManager`, `StreamingDestinationAdapter`, `DestinationTunnelCoordinator`, exploratory builds, SSU2 daemon/router delivery, or direct outbound cells.

On success set:

```text
plan_209 = passed-m10-product-only-remote-http-and-irc-application-interop
plan_181 = passed-m10-independent-application-and-service-interop-via-plan209
plan_195 = evidence-passed-m10-remote-independent-service-via-plan208-and-plan209-pending-plan204
milestone10_remote_service_interop = evidence-passed-via-plan208-and-plan209-pending-plan204
```

Final milestone closure remains owned by Plan 204 after the Java branch also closes.
