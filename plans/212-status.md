# Plan 212 status — M10 router-backed service-Destination material and canonical inbound Streaming

Status: **`registered-executable-m10-router-backed-service-destination-and-canonical-inbound-streaming-corrective`**.

Plan of record: [`212-m10-router-backed-service-destination-material-and-canonical-inbound-streaming-corrective.md`](212-m10-router-backed-service-destination-material-and-canonical-inbound-streaming-corrective.md).

Registration source head: `2d5408baaa25aec09c42ec9ebd719e4dcf302af8`.

## Why Plan 212 exists

A post-Plan-211 source audit found that the remaining M10 gap is still a product defect, not only an external-environment evidence gap:

- `ServiceTunnelManager::create_bridge_for_spec` still creates the service bridge through `SamLocalProductFabric::prepare_for_destination`; that fabric explicitly owns synthetic localhost-only tunnel material and is not live router transport.
- Plan 210 removed the explicit `dummy_outbound_tunnel()` swap, but the bridge's `DestinationOutboundRole` and local LS2 used by remote composition are still populated from the local fabric.
- `ServiceProduct::dial_and_bootstrap` builds real exploratory outbound/inbound material but does not install that material onto the owning service Destination runtime.
- the real inbound receive tunnel ids are not automatically bound to service runtimes as part of production provisioning.
- recovered Garlic enters `DestinationDispatcher`, but authenticated destination payloads are not drained through `pop_payload` and `StreamingDestinationAdapter::receive` into the canonical service `StreamingManager`.
- router bootstrap still carries one application `destination_hash`, which cannot represent independent HTTP and IRC service Destinations in one product instance.

## Current authority

```text
plan_208 = retained-partial-production-call-graph-corrective
plan_209 = retained-partial-black-box-composition-harness
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-blocked-by-plan212
plan_212 = registered-executable

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = not-yet-passed
m10_generic_remote_product = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## Execution graph

```text
M10:
  plan212
    -> generic router-backed Direction A + Direction B external proof
    -> rerun retained Plan 211 HTTP/IRC product lane
    -> M10 closure only if all remote rows are green

Java/M6:
  independent parallel branch

Later:
  closed Java branch + already-closed M10 -> Plan 204 convergence
```

Do not execute Milestone 11 planning as an M10-successor until Plan 211 is requalified after Plan 212 and `milestone10_final_acceptance` is honestly closed.
