# Plan 208 status — M10 production delivery-driver remote-route integration corrective

Status: **`passed-m10-production-delivery-driver-remote-route-integration`**.

Plan of record: [`208-m10-production-delivery-driver-remote-route-integration-corrective.md`](208-m10-production-delivery-driver-remote-route-integration-corrective.md).

Plan 208 closes the production-call-graph defect after Plans 202 and 206. Plan 206 retained useful executable backend primitives, but the normal Plan 182 service delivery driver still terminated a non-co-owned peer at the local `lookup_by_peer_hash() -> None -> unknown_peer` branch rather than invoking the remote route. Plan 208 modifies the production `ServiceTunnelManager::deliver_outbound` sweep itself, makes `route_outbound_remote_request` perform a real send through the existing `StreamingDestinationAdapter` + `deliver_outbound_cells` + `RouterDeliveryService` chain, and retains the inbound-owner registry that wires inbound data to the actual owning service runtime.

Closed authority transitions:

```text
plan_202 = retained-partial-routing-capability-surface-superseded-by-plan206
plan_206 = retained-partial-executable-backend-seams-superseded-by-plan208
plan_208 = passed-m10-production-delivery-driver-remote-route-integration
m10_remote_transport_core = passed-via-plan208
```

## What landed

- The production `ServiceTunnelManager::deliver_outbound` sweep
  (`crates/i2pr-daemon/src/service_tunnels.rs`) now invokes
  `route_outbound_remote_request` on the local-miss branch (Plan
  208 §A). A reachable remote peer no longer dies at the
  pre-Plan-208 terminal `unknown_peer` branch — the typed remote
  route is invoked first, the typed `route_outbound_remote_request`
  composes the queued request through the existing
  `StreamingDestinationAdapter`, and the typed
  `remote_outbound_composed` / `remote_outbound_requests`
  counters advance through the production code path.
- `ServiceTunnelManager::route_outbound_remote_request`
  (`crates/i2pr-daemon/src/service_tunnels.rs:1550`) now performs a
  real send (Plan 208 §B): the cached LeaseSet2 install, the
  adapter composition (`StreamingDestinationAdapter::send` →
  `OutboundDeliveryPlan.cells`), the encoding through the existing
  `deliver_outbound_cells` helper, and the dispatch through the
  existing `RouterDeliveryService::deliver`. The same
  `DestinationRouting` + `EciesSessionManager` swap-and-restore
  pattern that `bridge_to_peer` uses for the local bridge is
  applied here so the production bridge state never diverges.
- The Plan 206 inbound-owner registry and
  `dispatch_inbound_to_owned_destination` seam are retained and
  exercised through the seven new manager-level `plan208_g_*` rows
  in `crates/i2pr-daemon/src/service_tunnels.rs::plan208_remote_route_integration_tests`.
- The new `m10_remote_route_integration_through_deliver_outbound`
  external driver (`crates/i2pr-daemon/tests/service_tunnels_remote_route_integration_qualification.rs`)
  is `#[ignore]`-gated and exercises the production sweep
  against the exact-pinned i2pd 2.61.0 cache.
- The static checker
  `scripts/check-service-tunnel-acceptance-evidence.sh` is extended
  with Plan 208 §15 source-level invariants: production
  `deliver_outbound` must call `route_outbound_remote_request`,
  the `plan208_remote_route_integration_tests` module must
  exist, `compose_remote_cells` must be present, the counted Plan
  208 driver must not construct a parallel
  `StreamingManager::new` / `StreamingDestinationAdapter::new`,
  and the counted driver must never log peer key material.

## Stop conditions respected

- The Plan 208 driver never constructs a parallel
  `StreamingManager` / `StreamingDestinationAdapter` / router
  stack (Plan 208 §14 anti-shadow rule, enforced by the static
  checker).
- The Plan 208 driver never calls
  `record_remote_application_observation` (Plan 207 §9 forbids
  manual label injection; the static checker extends the same
  rule).
- The Plan 208 driver never logs peer key material (Plan 207 §7,
  enforced by the static checker).

## Acceptance

The lane is `#[ignore]`-gated. Without the exact-pinned external
i2pd environment, the test fails closed. With it, the driver
exercises the production sweep through the real Plan 184–193
router stack and verifies the typed counter advance.

The remaining work on the M10 closure path is Plan 209 (cleanup
of the Plan 207 application driver to remove the synthetic label
injection pattern) and Plan 204 (final authority / CI
normalization pass). Both stay blocked on the Plan 201 Java
mandatory rows per `plans/204-status.md`.

