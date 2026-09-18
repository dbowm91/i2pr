# Plan 206 status — M10 production remote delivery composition corrective

Status: **`retained-partial-executable-backend-seams-superseded-by-plan208`**.

Plan of record: [`206-m10-production-remote-delivery-composition-corrective.md`](../../implementation/service-tunnels/206-m10-production-remote-delivery-composition-corrective.md).

## Retained work

Plan 206 landed useful production primitives and those should be reused rather than rewritten:

- `RemoteDestinationBackend` carries the shared `DestinationTunnelCoordinator` and router delivery service;
- `ServiceDestinationDelivery::with_backend` / `has_backend` distinguish an executable backend from the historical marker object;
- `routing_decision_for` no longer returns `RemoteRouter` merely because a marker capability exists;
- typed manager seams exist for remote LeaseSet2 resolution, outbound remote requests, inbound owner registration, and inbound owner lookup/dispatch;
- `remote_lookup_cache_hit`, `remote_outbound_composed`, and `remote_inbound_dispatched` cannot be advanced through the generic `record_observation` helper;
- local co-owned routing remains separate.

## Why Plan 206 is not a pass

Source-level audit after execution found that the normal Plan 182 service delivery driver still follows the local-only branch:

```text
drain service-owned TransportSendRequest
 -> sam_destinations.lookup_by_peer_hash(...)
 -> Some(peer): bridge_to_peer(...)
 -> None: unknown_peer += 1; terminate_failed_delivery(...)
```

The production delivery sweep does not invoke `route_outbound_remote_request` on the non-local miss. The new remote seam is therefore present and useful, but it is not the normal product call path.

The same issue exists inbound: `dispatch_inbound_to_owned_destination` proves an ownership lookup/counter surface, but final acceptance still requires the actual recovered inbound destination/Streaming message to be fed into the owning service runtime.

Plan 206 tests and the prior external driver prove adjacent components and typed seams; they do not prove that an actual service listener drains its own Streaming queue through the remote backend.

## Corrected authority

```text
plan_202 = retained-partial-routing-capability-surface
plan_206 = retained-partial-executable-backend-seams-superseded-by-plan208
plan_208 = registered-executable-m10-production-delivery-driver-remote-route-integration
m10_remote_transport_core = not-yet-passed
```

Do not delete the Plan 206 backend API unless Plan 208 demonstrates a concrete reason to change it. Plan 208 owns the narrow integration into the real production delivery driver and the real inbound service runtime.
