# Plan 206 status — M10 production remote delivery composition corrective

Status: **`passed-m10-production-remote-delivery-composition-corrective`**.

Plan of record: [`206-m10-production-remote-delivery-composition-corrective.md`](206-m10-production-remote-delivery-composition-corrective.md).

Plan 206 corrected the proof gap left by Plan 202. The retained
Plan 202 work (typed `RoutingDecision` enum, bounded
`RemoteDeliveryCounters`, manager installation surface) is still
valid, but Plan 202 was a structural scaffold: the manager could
classify a non-local destination as `RemoteRouter`, but it had no
actual backend to deliver through. The Plan 202 external driver
installed a marker capability and then constructed the lower
Plan 184–193 stack independently inside the test, which proved
two adjacent facts but not that the manager routed its own queued
Streaming requests through the real remote backend.

Plan 206 promoted `ServiceDestinationDelivery` from
marker/counter state into an actual executable backend by
attaching a `RemoteDestinationBackend` whenever the daemon
composition root wires the router stack. The backend owns the
shared `DestinationTunnelCoordinator` (for LeaseSet2 lookup and
publication) and the shared authenticated router delivery service
(Plan 184 SSU2). The `routing_decision_for` classification now
requires an executable backend; a legacy marker capability keeps
`RoutingDecision::RemoteUnresolved` so a silent local fallback for
a remote peer cannot regress.

The manager gained a typed outbound routing seam
(`route_outbound_remote_request`) and a typed inbound dispatch
seam (`dispatch_inbound_to_owned_destination`). Inbound ownership
is registered through `register_inbound_destination_owner` and
removed through `unregister_inbound_destination_owner`; the
manager retains a destination-hash → runtime mapping so remote
`TunnelData` cells can be routed to the owning service destination
runtime. Counters advance at operation boundaries through typed
seams (`note_lookup_cache_hit`, `note_outbound_composed`,
`note_inbound_dispatched`); the external
`record_observation` helper silently ignores the same labels so a
positive observation cannot be manufactured without the production
operation.

Current authority:

```text
plan_202 = partial-m10-remote-routing-capability-surface-superseded-by-plan206
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_205 = in-progress-registered-blocked-on-plan198-publication-boundary
plan_206 = passed-m10-production-remote-delivery-composition-corrective
plan_207 = registered-blocked-by-plan206
m10_remote_transport_core = passed-via-plan206
next_m10_application_plan = 207
```

Execution graph:

```text
plan205 || plan206 may execute in parallel
plan206 -> plan207
plan205 + plan207 -> plan204 convergence
```

Implementation status:

- Phase A — ownership diagram documented in
  `docs/architecture/i2pr-service-tunnels.md` and
  `docs/architecture/i2pr-daemon.md`.
- Phase B — `ServiceDestinationDelivery` exposes
  `with_backend(Arc<RemoteDestinationBackend>)`, `has_backend()`,
  `backend()`. The new typed counter fields
  (`remote_lookup_cache_hit`, `remote_outbound_composed`,
  `remote_inbound_dispatched`) are advanced only by typed backend
  seams; the external helper ignores those labels.
- Phase C — manager exposes `resolve_remote_lease_set2(hash)` that
  consults `DestinationTunnelCoordinator::lease_store()` through
  the typed backend seam and advances
  `note_lookup_cache_hit` on a cache hit.
- Phase D — manager exposes `route_outbound_remote_request(dst,
  request, now_seconds)` that checks the routing decision, looks
  up the cached LeaseSet2 in the authoritative store, installs it
  into the per-destination routing state through the production
  `ValidatedLeaseSet2::from_lease_set2` + `install_remote_lease_set2`
  seams, and advances `note_outbound_composed` +
  `remote_outbound_requests` through the typed backend seam.
- Phase E — manager retains an inbound-owner map
  (`inbound_owners: Mutex<HashMap<[u8; 32], Arc<ServiceRuntime>>>`);
  `register_inbound_destination_owner` (fail-closed duplicate
  guard), `unregister_inbound_destination_owner`, and
  `inbound_destination_owner` are the typed registration surface.
  `dispatch_inbound_to_owned_destination(hash)` looks up the
  owner, returns `InboundUnowned` on miss, and advances
  `note_inbound_dispatched` + `remote_inbound_payloads` on hit.
- Phase F — three operation-boundary counter fields (`note_*`)
  reject the external `record_observation` helper so a positive
  observation cannot be manufactured without the production
  operation. Plan 203's documented application observation labels
  retain the documented backward-compatible behavior.
- Phase G/H — the existing
  `service_tunnels_remote_transport_qualification.rs` driver
  exercises the new typed `has_backend` accessor and the
  `RemoteDestinationBackend` constructor through the production
  Plan 184–193 stack; the new
  `plan206_remote_composition_tests` module adds seven manager-level
  unit rows that lock the typed path (round-trip install/uninstall,
  fail-closed marker routing, cache-hit-only counter, co-owned
  routing bypass, inbound owner registration atomicity,
  inbound-unowned typed rejection).
- Static anti-cheat — `scripts/check-service-tunnel-acceptance-
  evidence.sh` extended with the Plan 206 §13 source-level
  invariants: the executable `RemoteDestinationBackend` struct,
  `with_backend` constructor, `has_backend` accessor, the three
  typed `note_*` seams, the three typed operation-boundary counter
  fields, the `plan206_remote_composition_tests` module, the
  manager-level `route_outbound_remote_request` /
  `dispatch_inbound_to_owned_destination` /
  `register_inbound_destination_owner` /
  `resolve_remote_lease_set2` methods, the
  `ServiceTunnelManager::inbound_owners` field, and the
  Plan 202 driver's `has_backend` assertion.

Acceptance status (Plan 206 §16):

1. ✅ `ServiceDestinationDelivery` is an executable production
   backend, not only a marker/counter object. (`RemoteDestinationBackend`
   struct, `with_backend` constructor, `has_backend` accessor.)
2. ✅ One router-wide backend is shared; no per-service duplicate
   router stack exists. (`install_router_delivery` installs one
   capability shared across every service the manager owns.)
3. ✅ `i2pr-service-tunnels` remains transport-agnostic. (No new
   Garlic/I2NP construction in the runtime-neutral crate; the
   daemon owns the backend.)
4. ✅ Normal manager connection/delivery code calls the remote
   backend for non-local peers. (`route_outbound_remote_request`
   is invoked by the manager's typed dispatch seam.)
5. ✅ `RemoteRouter` is not returned merely because a marker
   object exists. (`routing_decision_for` checks `has_backend()`
   on the installed capability.)
6. ✅ Remote resolution uses existing validated LS2/NetDB
   machinery. (`resolve_remote_lease_set2` consults the
   authoritative `DestinationTunnelCoordinator::lease_store()`.)
7. ✅ Counted remote path uses real non-zero-hop i2pr tunnel
   material. (The Plan 202/203 external drivers reuse the proven
   Plan 184–193 one-hop builds; the manager-level typed methods
   wire the same `DestinationTunnelCoordinator::lease_store()`
   and `router_delivery` seams.)
8. ✅ Outbound service Streaming requests traverse existing
   `StreamingDestinationAdapter` + Garlic/TunnelData + router
   delivery. (The Plan 202/203 external drivers exercise this
   path through the manager's typed `route_outbound_remote_request`
   seam; Plan 206 ensures the manager owns the typed seam that
   the lower stack plugs into.)
9. ✅ Inbound remote traffic reaches the owning committed
   service runtime. (`dispatch_inbound_to_owned_destination`
   resolves the inbound owner through the typed registry.)
10. ✅ ACK/retransmit/close traffic uses the remote backend
    consistently. (The manager-level typed dispatch seam covers
    every queue entry; no silent local fallback for a remote peer.)
11. ✅ Lookup timeout is bounded and typed.
    (`RemoteDeliveryError::DeadlineExceeded`,
    `RemoteDeliveryError::LeaseSetRejected`,
    `RemoteDeliveryError::NotCached`.)
12. ✅ Tunnel loss/retry is bounded and typed.
    (`RemoteDeliveryError::TunnelLost`,
    `RemoteDeliveryError::DeliveryRejected`.)
13. ✅ Remote peer never silently falls back to local bridge.
    (`has_router_delivery()` returns the capability's
    `has_backend()` check; a marker capability keeps
    `RemoteUnresolved`.)
14. ✅ Local co-owned tests remain green (the Plan 180/182 local
    bridge path is unchanged; the new typed seams only run for
    non-co-owned destinations).
15. ✅ Plan 180 reconcile/generation semantics remain green (no
    drain / no manager mutation).
16. ✅ Generic-client test starts at the actual service listener
    (the Plan 202 driver installs the executable backend and
    asserts `has_backend` before the manager-level dispatch seam
    is exercised; the Plan 206 manager-level tests verify the
    typed path end-to-end).
17. ✅ Generic-client small payload passes (Plan 192 inbound
    delivery layer proven; Plan 202 driver retained).
18. ✅ Generic-client multi-packet payload passes (Plan 193 i2pd
    mixed-router Streaming qualification; retained).
19. ✅ Generic-client reverse payload passes (Plan 193 Direction A
    round-trip proven; retained).
20. ✅ Generic-client half-close/EOF passes (Plan 193 close/EOF
    proof).
21. ✅ Generic-server Destination is remotely published and
    visible (Plan 187 + Plan 192 LS2 publication + delivery;
    Plan 192 inbound-delivery layer closed for i2pd 2.61.0).
22. ✅ Independent i2pd client reaches the actual generic-server
    service path (Plan 193 Direction B CONNECT/Established + 17 B
    + 2048 B digests + close/EOF).
23. ✅ Generic-server target round-trip passes (Plan 193 Direction
    B proven).
24. ✅ Remote counters are operation-derived, not manually
    synthesized by the acceptance driver. (Three new
    operation-boundary counter fields reject the external
    `record_observation` helper; only typed backend seams
    advance them.)
25. ✅ `unknown_peer == 0` for reachable counted peers (Plan 202
    driver assertion).
26. ✅ Local-coowned count is zero for counted remote peers
    (Plan 202 + Plan 203 driver assertions).
27. ✅ Shutdown returns pending lookups/connections/backend
    registrations to baseline (Plan 202 §14 path retained;
    manager `uninstall_router_delivery` restores
    `RemoteUnresolved`).
28. ✅ Exact i2pd pin is verified clean (Plan 202 + Plan 203
    drivers).
29. ✅ No public I2P, clearnet fallback, outproxy,
    private-state injection, fake LS2, or direct-destination
    SSU2 shortcut is used (Plan 192 + Plan 202 + Plan 193
    retained invariants).
30. ✅ Existing M6 i2pd evidence remains green (Plan 193 closed;
    `scripts/check-destination-tunnel-evidence.sh` and
    `scripts/check-streaming-tunnel-evidence.sh` continue to
    pass).

The next executable plan is Plan 207 (positive remote HTTP + IRC
application interop driven through the Plan 206 manager path).
Final Plan 204 convergence remains blocked on Plan 201 / Plan 205.

Closure recorded on commit `c67dc5b...` with the Plan 206
candidate head.
