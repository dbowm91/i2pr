# Plan 212 status — M10 router-backed service-Destination material and canonical inbound Streaming

Status: **`in-progress-source-closure-landed-external-qualification-pending`**.

Plan of record: [`212-m10-router-backed-service-destination-material-and-canonical-inbound-streaming-corrective.md`](212-m10-router-backed-service-destination-material-and-canonical-inbound-streaming-corrective.md).

Registration source head: `2d5408baaa25aec09c42ec9ebd719e4dcf302af8`.

## Source closure (this head)

The Plan 212 §B–§K + §M + §N source work has landed:

- `RouterDestinationNetworkState` + bridge
  `install/clear/has/summary` + `compose_router_send` (explicit
  router-backed compose, fail-closed without/expired state, never
  fabric) + `install_remote_lease_set2_into_router_state` +
  `dispatch_router_garlic_to_canonical_streaming` (`dispatch` +
  `pop_payload` drain + `StreamingDestinationAdapter::receive`
  into the SAME canonical service `StreamingManager`) in
  `crates/i2pr-daemon/src/sam/streams.rs`;
- manager wrappers `install/clear/has/summary_service_router_material`,
  `dispatch_router_inbound_to_canonical_streaming` (wakes the
  existing delivery driver when `accepted > 0`),
  `remote_target_hash_for_reference` (Base32/Configured/Alias,
  local co-owned stays local), `spec_reference_for_service`,
  `spec_is_server`, plus the `route_outbound_remote_request` /
  `compose_remote_cells` migration to `compose_router_send` in
  `crates/i2pr-daemon/src/service_tunnels.rs`;
- `ServiceProduct::start` reorder (router-only bootstrap, then
  `prepare()`, then per-service real outbound/inbound provisioning
  with disjoint `Plan212TunnelIdAllocator` ids, real LS2 from real
  `InboundLeaseSource`, owner registration, per-service lookup via
  `resolve_remote_destination_for_service`, server publication via
  `publish_service_ls2_for_service`, then supervisors; atomic
  failure), router-only `ReferencePeer` (no application hash),
  standard-first/short-fallback `decode_inbound_ssu2_i2np`, and the
  `streaming_packets_accepted`-gated inbound counter in
  `crates/i2pr-daemon/src/service_product.rs`;
- 25 `plan212_*` manager-level unit rows;
- static checker Plan 212 §20–§26 invariants;
- `#[ignore]`-gated
  `service_tunnels_plan212_router_backed_product.rs` generic
  Direction A/B driver (production API only, no shadow stack,
  documented §21 keys) + runner prerequisite integration;
- Plan 211 driver mechanically adapted to the router-only
  `ReferencePeer` (targets from spec destinations).

Routine CI/static floors are green on this head. The external
generic Direction A + Direction B lane against exact-pinned
unmodified i2pd 2.61.0 has NOT been run here and is NOT claimed;
Plan 212 stays `in-progress` until that lane passes, Plan 211 is
requalified after it, and only then may M10 close.

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
plan_212 = in-progress-source-closure-landed-external-qualification-pending

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
