# Plan 208 status — M10 production delivery-driver remote-route integration corrective

Status: **`retained-partial-production-call-graph-corrective-superseded-by-plan210`**.

Plan of record: [`208-m10-production-delivery-driver-remote-route-integration-corrective.md`](../../implementation/service-tunnels/208-m10-production-delivery-driver-remote-route-integration-corrective.md).

## Retained value

Plan 208 corrected a real production call-graph defect: `ServiceTunnelManager::deliver_outbound` no longer terminates every non-co-owned queued Streaming request at the local `unknown_peer` branch. The normal delivery sweep can invoke `route_outbound_remote_request`, and the remote branch reuses `StreamingDestinationAdapter`, `deliver_outbound_cells`, and `RouterDeliveryService`.

Those changes are retained and must not be reverted.

## Why Plan 208 is no longer remote-transport closure authority

A later source-level audit found that the remote branch can still compose with service bridge material originating from `SamLocalProductFabric`, including the local bridge LeaseSet2/outbound role and the `dummy_outbound_tunnel()` placeholder swap. That proves the routing call graph exists, but not that a normal M10 service Destination owns real destination tunnel material.

The inbound-owner surface also does not yet prove the complete remote path: raw inbound tunnel traffic must be routed from its receiving tunnel id to the owning service, recovered Garlic must be decrypted by that service's dispatcher/session state, and protocol-6 payload must reach the same canonical service `StreamingManager` used by the application socket pump.

Plan 210 owns those missing product requirements.

## Corrected authority

```text
plan_202 = retained-partial-routing-capability-surface
plan_206 = retained-partial-executable-backend-seams
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_210 = registered-executable-m10-real-service-destination-network-material-and-inbound-streaming
m10_remote_transport_core = not-yet-passed
m10_generic_remote_product = not-yet-passed
```

Do not remove Plan 208 tests/counters unless Plan 210 replaces them with strictly stronger production-boundary coverage.
