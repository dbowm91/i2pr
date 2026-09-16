# Plan 210 status — M10 service-Destination structural corrective

Status: **`retained-partial-structural-corrective-superseded-by-plan212`**.

Plan of record: [`210-m10-real-service-destination-tunnel-material-and-inbound-streaming-corrective.md`](210-m10-real-service-destination-tunnel-material-and-inbound-streaming-corrective.md).

Plan 210 landed useful structural work, but its prior `passed-m10-real-service-destination-network-material-and-inbound-streaming` interpretation was too strong and is superseded by Plan 212.

## Retained work

Plan 210 validly landed:

- receive-tunnel-id -> service runtime ownership APIs;
- removal of the explicit `dummy_outbound_tunnel()` swap from remote composition;
- actual remote Destination hash lookup input instead of `SHA256(router.info)`;
- recovered Garlic entering `DestinationDispatcher`;
- typed remote inbound/outbound operation counters;
- static checks covering those structural properties.

## Remaining defect discovered after Plan 211

The service runtime is still created through `SamLocalProductFabric::prepare_for_destination`, whose outbound/inbound tunnel material is explicitly synthetic and localhost-only. The real exploratory outbound/inbound pair built by `ServiceProduct` is not installed as service-Destination-owned network material.

In addition:

- production provisioning does not bind the real inbound receive ids to the owning service runtime;
- inbound Garlic processing stops before `DestinationDispatcher::pop_payload` -> `StreamingDestinationAdapter::receive` -> canonical service `StreamingManager`;
- router bootstrap still represents one application Destination hash instead of performing per-service target lookup.

Therefore Plan 210 is retained as a prerequisite structural corrective, not remote transport closure authority.

## Corrected authority

```text
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-blocked-by-plan212
plan_212 = registered-executable
m10_remote_transport_core = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

See `plans/212-status.md` for the executable handoff.
