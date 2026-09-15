# Plan 208 — M10 production delivery-driver remote-route integration corrective

Status: **registered / executable**.

## 1. Purpose

Plan 208 corrects the remaining production-call-graph defect after Plans 202 and 206.

Plan 206 added useful real primitives:

- `RemoteDestinationBackend`,
- `ServiceDestinationDelivery::with_backend`,
- `routing_decision_for`,
- `resolve_remote_lease_set2`,
- `route_outbound_remote_request`,
- inbound-owner registration and `dispatch_inbound_to_owned_destination`,
- operation-boundary counters that cannot be advanced through the generic observation helper.

Those primitives are retained.

However, the normal M10 product loop is still the Plan 182 local/co-owned delivery driver. It drains queued `TransportSendRequest`s, looks up the peer in the manager-owned `SamDestinations` mirror, and on a miss records `unknown_peer` and terminates the request. The production driver does not call the Plan 206 remote route on that miss. Plan 206 therefore landed an executable seam, but not the required integration into the normal product path.

Plan 208 must wire the already-existing remote backend into that existing delivery driver. It must not create another delivery loop, another per-service router stack, or another acceptance-only Streaming stack.

## 2. Scope

This is a narrow production-composition corrective.

Primary code surface:

- `crates/i2pr-daemon/src/service_tunnels.rs`
- `crates/i2pr-daemon/src/service_delivery.rs`
- the existing daemon composition root that owns `DestinationTunnelCoordinator` and `RouterDeliveryService`
- existing inbound destination/tunnel dispatch code from Plans 184–193
- focused tests/checkers needed to prove this call graph

Expected supporting changes are acceptable in:

- `crates/i2pr-daemon/src/destination_tunnels.rs`
- `crates/i2pr-daemon/src/destination_streaming.rs`
- `crates/i2pr-daemon/src/router_i2np.rs`
- existing service-tunnel integration harness files

Do not move transport/protocol logic into `i2pr-service-tunnels`.

## 3. Non-goals

Plan 208 does **not**:

- redesign Streaming,
- add another I2P transport,
- patch i2pd or Java I2P,
- use public I2P,
- add direct-destination SSU2 as a shortcut,
- copy private router/client state,
- add a second `StreamingManager` solely for qualification,
- add another Python protocol harness,
- close the HTTP/IRC application rows; Plan 209 owns those,
- close Plan 204 or the Java second-family branch.

## 4. Required architecture after this plan

The normal M10 path must be exactly one routing decision inside the existing manager-owned delivery driver:

```text
local TCP / profile connection
  -> service-owned StreamingManager queues TransportSendRequest
  -> existing ServiceTunnelManager destination delivery driver
       |
       +-- peer is manager co-owned
       |     -> existing Plan 182 bridge_to_peer local path
       |
       +-- peer is non-local + executable remote backend installed
       |     -> ordinary remote LeaseSet2 resolution/cache
       |     -> existing service-owned Streaming routing/session state
       |     -> StreamingDestinationAdapter
       |     -> Garlic/TunnelData
       |     -> existing real outbound tunnel material
       |     -> router-owned RouterDeliveryService / SSU2
       |
       +-- peer cannot be resolved/routed
             -> bounded typed failure; no local fallback
```

Inbound is the reverse of the same product ownership:

```text
router-owned SSU2/I2NP inbound
  -> existing tunnel/Garlic recovery
  -> destination hash / destination delivery target
  -> ServiceTunnelManager inbound-owner registry
  -> owning service DestinationRuntime / StreamingManager
  -> existing profile/server pump
  -> loopback target
```

There must be one daemon-wide remote router context shared by services. The service-tunnel manager consumes typed handles; it does not own another SSU2 service or NetDB.

## 5. Phase A — make the existing delivery driver route remote misses

Locate the production function that currently drains queued requests (`run_service_delivery_driver` and its sweep helper/current equivalent).

The existing local path must remain unchanged when `lookup_by_peer_hash()` resolves a co-owned peer.

When the local peer lookup misses:

1. derive/use the same remote destination hash already associated with the queued request;
2. call the manager routing decision;
3. on `RoutingDecision::RemoteRouter`, invoke the real remote-delivery operation from the existing driver;
4. on `RoutingDecision::RemoteUnresolved`, terminate with a typed bounded failure;
5. do **not** increment the local `unknown_peer` result before the remote route has been attempted;
6. do **not** call `bridge_to_peer` for a non-local destination;
7. do **not** silently fall back to the local bridge if the remote route fails.

A reachable remote peer must therefore cease taking the old `lookup_by_peer_hash -> None -> unknown_peer -> terminate` branch.

### Required source-level invariant

The static checker must prove a call to the remote-routing seam exists in the normal delivery-driver implementation, not only in tests.

A grep-presence check for the method definition is insufficient. The checker should verify the call occurs in the production driver/sweep region and should reject a state where all calls are under `#[cfg(test)]` or test modules.

## 6. Phase B — make `route_outbound_remote_request` perform a real send

The Plan 206 method must represent an actual production operation, not only classification or LS2 installation.

For a remote request it must, directly or through one small production helper:

1. obtain the installed executable backend;
2. use the authoritative `DestinationTunnelCoordinator`/LeaseSet2 store;
3. if a valid current LeaseSet2 is cached, reuse it;
4. if no valid current LeaseSet2 is cached, initiate/drive the existing bounded ordinary lookup path rather than inventing a test-only LS2;
5. validate/install the resulting LeaseSet2 into the service destination's existing routing state;
6. use the service destination's existing Streaming/ECIES/outbound role state;
7. compose the queued request through the existing `StreamingDestinationAdapter`;
8. encode/deliver the resulting cells using the existing outbound tunnel/router-delivery path;
9. return a typed success/failure that lets the caller preserve Streaming retransmit/close semantics.

Do not duplicate the send algorithm currently exercised in the Plan 193/207 tests. Move/reuse the smallest production helper if code needs to be shared.

### Important ownership rule

The request being sent must be the same request drained from the actual service-owned Streaming queue. A test-created `StreamingManager::drain_outbound()` does not satisfy this phase.

## 7. Phase C — remote resolution must be real and bounded

`resolve_remote_lease_set2` currently proves access to the authoritative cache. Extend the production path only as needed so a cache miss can use the existing ordinary lookup machinery.

Requirements:

- use existing `DestinationTunnelCoordinator` lookup state;
- reuse real floodfill selection and real tunnel/reply paths;
- bounded deadline;
- bounded number of concurrent resolutions;
- current LS2 validation/signature rules retained;
- no private-state injection;
- no fabricated LeaseSet2 fixture in a counted remote row;
- no direct transport fallback.

A cache hit may remain a fast path, but a counted test must not rely on manually pre-populating the service manager with peer-private state.

## 8. Phase D — wire the real inbound path to the owning service runtime

Plan 206's `dispatch_inbound_to_owned_destination` currently proves the manager can map a destination hash to an owning runtime. Plan 208 must connect that registry to the actual inbound data path.

After existing SSU2/I2NP/tunnel/Garlic recovery identifies a service-owned destination:

1. resolve the owner through the manager's inbound-owner registry;
2. feed the recovered destination/Streaming message into the owning destination runtime/dispatcher using the existing destination APIs;
3. allow the service-owned `StreamingManager` to emit ACK/data/close responses through the same normal outbound driver;
4. increment `remote_inbound_dispatched` only after successful production dispatch;
5. return `InboundUnowned`/typed failure on a destination not owned by the active generation.

Returning `Arc<ServiceRuntime>` and incrementing a counter without delivering the recovered message is not acceptance.

If the current router inbound composition root cannot reach the manager cleanly, introduce one narrow typed callback/registry handle at the daemon layer. Do not make `service_delivery.rs` a second inbound router.

## 9. Phase E — generation and lifecycle correctness

Inbound ownership must follow Plan 180 generation semantics:

- register only committed/live service destinations;
- reject duplicate ownership atomically;
- on reconcile, install replacement ownership at the correct commit boundary;
- retain the old owner only for the documented draining interval if required by existing semantics;
- remove ownership when the generation drains/shuts down;
- pending remote resolutions are cancelled/bounded on shutdown;
- no stale destination owner may receive later inbound data.

## 10. Phase F — operation-derived evidence only

The decisive counters for Plan 208 are:

- `remote_lookup_cache_hit` and/or a new typed ordinary-lookup-success counter if required,
- `remote_outbound_composed`,
- `remote_inbound_dispatched`.

They must advance only from production operation boundaries.

The counted driver must not call:

- `record_observation("remote_outbound_composed")`,
- `record_observation("remote_inbound_dispatched")`,
- an equivalent test-only note function,
- a free-form application observation helper to manufacture transport success.

Legacy Plan 202/203 counters may remain for compatibility but cannot be used as the sole proof for Plan 208 acceptance.

## 11. Phase G — focused unit and integration tests

Add focused unit tests for the call graph, not just the new method surface.

Minimum unit coverage:

1. local co-owned request still uses the local bridge and does not touch remote backend;
2. remote request with executable backend does not terminate at `unknown_peer` before remote routing;
3. marker-only capability remains `RemoteUnresolved`;
4. missing/expired LS2 produces bounded typed failure;
5. successful remote composition increments `remote_outbound_composed` exactly from the production send;
6. inbound recovered message dispatches into the registered destination owner and increments `remote_inbound_dispatched`;
7. unowned inbound destination rejects;
8. reconcile/shutdown removes stale inbound ownership.

Tests may provide deterministic fake router-delivery handles at the narrow I/O boundary, but they must exercise the same production delivery driver functions.

## 12. Phase H — external generic-client product proof

Reuse the exact-pinned i2pd 2.61.0 controlled loopback lane.

The i2pr side must begin at the actual generic-client listener created by `ServiceTunnelManager`.

Required flow:

```text
test TCP client
 -> actual i2pr generic-client listener
 -> actual service-owned Streaming queue
 -> actual production delivery driver
 -> Plan 208 remote backend
 -> real one-hop i2pr tunnel/router delivery
 -> i2pd-owned Destination/server tunnel
 -> loopback echo fixture
```

Required observations:

- small payload digest equality;
- payload larger than one Streaming packet;
- reverse data;
- half-close/EOF behavior;
- `unknown_peer == 0` for the reachable peer;
- `local_coowned_deliveries == 0` for the remote peer;
- production `remote_outbound_composed > 0`;
- production inbound dispatch counter > 0 when the reverse stream is received;
- no direct destination-over-SSU2 shortcut.

The acceptance driver must not construct a second i2pr `StreamingManager` to make this row pass.

## 13. Phase I — external generic-server product proof

Reuse the existing independent-client/i2pd capability proven in the M6 work.

Required flow:

```text
independent i2pd/SAM client
 -> ordinary I2P lookup/connect
 -> i2pr-published service Destination
 -> real inbound tunnel/Garlic path
 -> ServiceTunnelManager inbound owner
 -> actual generic-server Streaming runtime
 -> loopback echo fixture
 -> reply through same production remote outbound path
```

Required observations:

- service Destination is published/visible through the ordinary controlled topology;
- independent client establishes Streaming;
- small payload round-trip;
- multi-packet payload round-trip;
- reverse data;
- close/EOF;
- fixture observes the expected bytes;
- `remote_inbound_dispatched > 0` from the real inbound path;
- reply causes `remote_outbound_composed > 0`;
- no test-local duplicate router stack on the i2pr side.

## 14. Anti-shadow-stack rule

The counted Plan 208 acceptance test on the i2pr side must not directly construct or drive a parallel:

- `StreamingManager`,
- `StreamingDestinationAdapter`,
- `DestinationTunnelCoordinator`,
- `ExploratoryBuildCoordinator`,
- `Ssu2DaemonService`,
- `RouterDeliveryService`,

except through the same daemon/product composition helper that production uses to install the one shared remote context.

The peer/reference side may naturally use i2pd/SAM and fixture code.

If an integration harness must create the daemon-owned remote context because the full binary cannot be booted conveniently, expose/use one production composition function and pass the resulting manager/context into the product. Do not reproduce the stack in the test.

## 15. Static checker changes

Extend `scripts/check-service-tunnel-acceptance-evidence.sh` with semantic guardrails:

- production service delivery driver contains a call to the remote routing seam;
- old `unknown_peer` miss branch is guarded so an executable remote route is attempted first;
- counted Plan 208 external driver does not import/directly call `StreamingDestinationAdapter::send`;
- counted Plan 208 driver does not create a standalone `StreamingManager`;
- counted Plan 208 driver does not call the generic observation helper for decisive transport counters;
- authoritative pass rows require operation-derived Plan 208 counters and command/fixture facts;
- local-only lane stays fail-closed for remote rows.

Do not rely only on token-presence checks that can be satisfied by comments or method definitions.

## 16. Acceptance criteria

Plan 208 passes only when **all** are true on one exact head:

1. Existing Plan 182 local/co-owned delivery remains green.
2. Normal production delivery driver invokes the remote route for non-local queued requests.
3. A reachable remote peer no longer dies at the pre-Plan-208 `unknown_peer` branch.
4. Remote route uses the service-owned queued request, not a test-created queue.
5. Remote resolution uses existing NetDB/LS2 machinery and is bounded.
6. Outbound bytes traverse existing StreamingDestinationAdapter + Garlic/TunnelData + real tunnel/router delivery from production code.
7. Inbound recovered remote data is delivered to the actual owning service destination runtime.
8. ACK/retransmit/close traffic uses the same remote route.
9. Generic-client small payload passes from the actual listener.
10. Generic-client multi-packet payload passes.
11. Generic-client reverse payload passes.
12. Generic-client half-close/EOF passes.
13. Independent generic-server client reaches the actual service destination.
14. Generic-server target round-trip passes.
15. `remote_outbound_composed > 0` is operation-derived.
16. `remote_inbound_dispatched > 0` is operation-derived.
17. Reachable remote peer has no local-coowned fallback.
18. Reachable remote peer has no terminal `unknown_peer` result.
19. Inbound owner registration follows generation/reconcile/shutdown semantics.
20. Counted i2pr acceptance path contains no parallel shadow Streaming/router stack.
21. Exact-pinned i2pd verification remains clean.
22. Existing M6/i2pd Plan 193 evidence remains green.
23. Routine workspace tests/static/dependency checks remain green.

Only then set:

```text
plan_206 = retained-partial-backend-seams-superseded-by-plan208
plan_208 = passed-m10-production-delivery-driver-remote-route-integration
m10_remote_transport_core = passed-via-plan208
next_m10_application_plan = 209
```

## 17. Stop conditions

Stop and record the first real boundary if:

- the existing service-owned Streaming state cannot expose enough information to compose a remote send without redesign;
- the daemon inbound path cannot identify the owning destination at the point needed for service dispatch;
- an ordinary LS2 lookup cannot complete in the controlled topology;
- an i2pd-independent client cannot reach the i2pr service Destination for a protocol reason;
- passing requires a direct-destination SSU2 shortcut or private-state injection.

Do not work around those boundaries with a second test-owned router stack. Record the exact boundary and create one narrower follow-up only if evidence requires it.

## 18. Handoff

Plan 208 is the next executable M10 plan. It may run in parallel with Plan 205's Java branch.

Execution graph:

```text
Java: 205 -> (evidence-driven successor only if 205 fails)
M10:  208 -> 209
Final: Java branch closed + 209 passed -> 204 convergence
```
