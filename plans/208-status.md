# Plan 208 status — M10 production delivery-driver remote-route integration corrective

Status: **`registered-executable-m10-production-delivery-driver-remote-route-integration`**.

Plan of record: [`208-m10-production-delivery-driver-remote-route-integration-corrective.md`](208-m10-production-delivery-driver-remote-route-integration-corrective.md).

Plan 208 supersedes the over-promoted closure interpretation of Plan 206. Plan 206 retained useful executable backend primitives, but the normal Plan 182 service delivery driver still terminates a non-co-owned peer at the local `lookup_by_peer_hash() -> None -> unknown_peer` branch rather than invoking the remote route.

Current authority:

```text
plan_202 = retained-partial-routing-capability-surface
plan_206 = retained-partial-executable-backend-seams-superseded-by-plan208
plan_207 = retained-partial-real-application-client-harness-superseded-by-plan209
plan_208 = registered-executable-m10-production-delivery-driver-remote-route-integration
plan_209 = registered-blocked-by-plan208
m10_remote_transport_core = not-yet-passed
m10_remote_application_interop = not-yet-passed
```

Plan 208 must modify the existing manager-owned delivery driver itself. The counted i2pr path may not construct a parallel `StreamingManager`/`StreamingDestinationAdapter`/router stack. A reachable remote peer must flow through the real service-owned Streaming queue and the shared Plan 206 backend; inbound recovered remote data must be delivered into the actual owning service runtime.

Execution graph:

```text
plan205 || plan208
plan208 -> plan209
closed Java branch + plan209 -> plan204 convergence
```

On success set:

```text
plan_208 = passed-m10-production-delivery-driver-remote-route-integration
m10_remote_transport_core = passed-via-plan208
next_m10_application_plan = 209
```
