# Plan 206 — M10 production remote delivery composition corrective

Status at registration: **registered-executable**.

Plan 206 corrects the over-promotion of Plan 202. Plan 202 landed useful routing classification, bounded counters, and external-driver scaffolding, but it did not satisfy its own production-composition contract: the normal `ServiceTunnelManager` data path still does not own/use the real remote LeaseSet2/tunnel/Streaming/router-delivery backend for non-local peers. The Plan 202 external driver installs a marker capability into the manager, asserts `RoutingDecision::RemoteRouter`, and then constructs/exercises the lower Plan 184–193 stack separately in the test.

This plan must turn that seam into a real production backend and prove it through ordinary service-tunnel I/O.

## 1. Goal

Make the existing M10 service product perform this path without a parallel test-owned router stack:

```text
ordinary loopback application socket
 -> ServiceTunnelManager client profile
 -> service-owned StreamingManager
 -> daemon-owned remote delivery backend
 -> validated Destination / DestinationHash
 -> authoritative Standard LeaseSet2 lookup/cache
 -> real destination tunnel material
 -> StreamingDestinationAdapter
 -> Garlic / TunnelData
 -> router-owned authenticated transport
 -> exact-pinned independent i2pd Destination/service
```

and the reverse path:

```text
independent i2pd Destination/service
 -> i2pr authenticated router transport
 -> registered inbound tunnel
 -> TunnelData reassembly
 -> destination Garlic / ECIES dispatch
 -> owning service DestinationRuntime / StreamingManager
 -> normal service socket pump
```

The Plan 182 local co-owned bridge remains valid for genuinely co-owned peers. It must not be a fallback for remote peers.

## 2. Dependency and execution graph

Plan 206 is independent of the Java Plan 205 branch and may execute in parallel with it.

```text
plan205 || plan206 may execute in parallel
plan207 depends on plan206
plan204 final convergence depends on plan205 + plan207
```

Retained inputs:

- Plan 193: exact-pinned i2pd mixed-router Streaming path is proven.
- Plans 184–192: authenticated router delivery, one-hop tunnel build/install, NetDB lookup/publication, inbound dispatch, reply-path and wire-format corrections exist.
- Plan 182: local M10 service delivery and application socket pumping are proven.
- Plan 202: `RoutingDecision`, `ServiceDestinationDelivery`, manager installation seam, bounded remote counters and external-driver scaffolding exist.

Do not discard those pieces. Correct the missing composition between them.

## 3. Exact defect to correct

At registration head `c67dc5b594fc32e48bcaee4bcbe8004d0d94d04b`:

1. `ServiceDestinationDelivery` is primarily a counter/resolution-table object. It does not itself carry or invoke the production `DestinationTunnelCoordinator`, router delivery service, destination tunnel pool/registry, Streaming adapter, or inbound ownership dispatcher.
2. `ServiceTunnelManager::routing_decision_for()` classifies a non-local destination as `RemoteRouter` when a capability is installed, even if no actual remote delivery operation can occur.
3. `service_tunnels_remote_transport_qualification.rs` installs that capability and then constructs the real lower-layer stack independently inside the test.
4. The test therefore proves two adjacent facts — manager classification and lower-layer Streaming interoperability — but not that the manager routes its own queued Streaming requests through the lower stack.
5. The original Plan 202 required actual generic-client and generic-server application I/O through the manager. That acceptance was not met.

Plan 206 must remove this proof gap.

## 4. Architectural rule

Keep `i2pr-service-tunnels` runtime-neutral. The remote backend belongs in `i2pr-daemon`.

Preferred shape:

```text
ServiceTunnelManager
  -> ServiceDestinationDelivery
       LocalCoOwned => existing Plan 182 bridge
       RemoteRouter => real daemon-owned RemoteDestinationBackend
```

`RemoteDestinationBackend` may be a concrete owned context, a small trait object, or a set of typed handles/callbacks. Choose the least abstract design that fits the existing daemon ownership model.

It must represent real executable operations, not merely readiness/classification.

Required logical operations:

```text
resolve_remote(destination, deadline)
begin_stream(service_destination, remote, ports, deadline)
send_streaming(service_destination, TransportSendRequest, deadline)
dispatch_inbound_tunnel_data(router/tunnel metadata, bytes)
expire/cancel(service_generation, deadline)
snapshot_counters()
```

Names may differ. The semantic contract may not.

## 5. Phase A — locate the real daemon owners

Before changing APIs, identify the existing long-lived owners for:

- authenticated SSU2/router delivery;
- `DestinationTunnelCoordinator` and authoritative RouterInfo/LeaseSet2 stores;
- exploratory/destination tunnel build/install state;
- `DataPlaneRegistry` / inbound tunnel route metadata;
- destination ECIES/Garlic dispatch;
- per-destination `StreamingManager` / `StreamingDestinationAdapter` state;
- cancellation and service graph ownership.

If these are currently only composed in external tests, add one router-wide daemon context/service that owns them. Do not instantiate one lower stack per M10 service or per connection.

Acceptance for this phase requires a documented ownership diagram in `docs/architecture/i2pr-service-tunnels.md` or `i2pr-daemon.md` that points to concrete production types/functions.

## 6. Phase B — make `ServiceDestinationDelivery` executable

Evolve the Plan 202 capability from marker/counter state into an actual remote backend handle.

Required properties:

- installing the capability means an executable backend is present;
- `RemoteRouter` may only be returned when the backend can attempt remote resolution/delivery;
- backend absence remains `RemoteUnresolved`;
- counters are advanced by backend operations, not arbitrary acceptance-driver calls;
- lookup/resolution tables remain bounded;
- no payload/private-key logging;
- no direct Destination-over-SSU2 counted shortcut.

The public `record_observation(label)` helper may remain for narrow unit diagnostics, but Plan 206 counted evidence must not use it to manufacture operation counters.

## 7. Phase C — remote resolution through the manager path

For a configured non-local Destination:

1. decode/validate public Destination material using existing codecs;
2. derive the destination hash;
3. consult the existing validated LeaseSet2 store;
4. when absent/stale, start a bounded ordinary lookup through the real `DestinationTunnelCoordinator`;
5. use a real registered inbound reply route from Plan 190;
6. dispatch lookup through a real outbound tunnel/router delivery;
7. ingest the returned Standard LS2 through existing validation;
8. expose the resulting `RemoteDestination` to the service Streaming path;
9. fail with a typed bounded error when any stage cannot complete.

The service manager may not call a test helper that independently reconstructs this flow.

## 8. Phase D — route actual manager Streaming requests remotely

Find the production code that currently drains each service destination's outbound Streaming requests (the Plan 182 local-delivery driver / equivalent).

Change the dispatch decision to:

```text
peer co-owned by committed generation
  -> existing bridge_to_peer local path
otherwise
  -> executable remote backend
     -> resolve/cache LS2
     -> StreamingDestinationAdapter::send
     -> outbound Garlic/TunnelData composition
     -> router-owned delivery
```

Mandatory properties:

- SYN, SYN response, ACK, retransmit, data, close and reset traffic all use the same backend decision;
- no silent local fallback for a remote peer;
- remote timeout/tunnel loss is typed and bounded;
- queue entries are acknowledged/removed only according to the existing Streaming transport contract;
- the normal service connection handler need not understand I2NP/SSU2.

## 9. Phase E — inbound ownership and dispatch

Wire the real router-wide inbound path into the owning M10 destination runtime.

Required chain:

```text
inbound TunnelData
 -> existing reassembly/validation
 -> destination Garlic/ECIES recovery
 -> destination identity/hash ownership lookup
 -> committed ServiceRuntime / DestinationRuntime
 -> StreamingDestinationAdapter::receive
 -> owning StreamingManager
 -> socket pump and/or queued response
```

Generation rules:

- a draining generation may finish already-owned connections according to Plan 180 policy;
- a replaced generation must not receive new traffic for the new identity;
- shutdown/reconcile must remove backend registrations and pending lookup/connect state;
- all maps/queues remain bounded.

## 10. Phase F — replace synthetic counter evidence

For Plan 206 qualification, forbid this pattern as a pass condition:

```text
capability.record_observation("remote_lookup_succeeded")
capability.record_observation("remote_stream_established")
```

before/without the corresponding production operation.

Counters must be advanced at the operation boundary itself. Tests may inspect them afterward.

Recommended invariants:

```text
remote_lookup_started >= 1
remote_lookup_succeeded >= 1
remote_stream_connect_started >= 1
remote_stream_established >= 1
remote_outbound_requests >= 2
remote_inbound_payloads >= 1
local_coowned_deliveries == 0 for the remote peer
unknown_peer == 0 for a reachable remote peer
pending resolutions == 0 after close
```

## 11. Phase G — positive generic-client product test

Use exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`) on loopback.

Required Direction A topology:

```text
ordinary TCP test client
 -> actual bound i2pr generic-client listener
 -> ServiceTunnelManager supervisor/connection path
 -> production remote backend
 -> i2pd-owned SAM/stock Destination
 -> deterministic loopback echo service
```

Counted evidence must include:

- actual listener socket accepted connection;
- manager routing decision remote;
- real LS2 lookup or validated cache hit with provenance;
- Streaming reaches `Established` through manager-owned state;
- small payload digest equality;
- payload larger than one Streaming packet digest equality;
- reverse response digest equality;
- half-close/EOF behavior;
- clean shutdown/resource baseline.

Do not directly instantiate/use an independent `StreamingManager` in the test as the counted client path. The counted path begins at the service listener.

## 12. Phase H — positive generic-server product test

Required Direction B topology:

```text
i2pd public SAM STREAM client / stock client
 -> published i2pr generic-server Destination
 -> real i2pr inbound tunnel + destination dispatch
 -> ServiceTunnelManager server Streaming accept
 -> configured loopback target
 -> response back through remote backend
 -> i2pd client
```

Counted evidence must include:

- service Destination has real non-zero-hop inbound/outbound tunnel material;
- Standard LS2 publication is acknowledged/visible to the independent router;
- independent client establishes Streaming;
- target receives exact bytes;
- response digest equality;
- clean close and no leaked registrations/tasks.

Direction B is mandatory for Plan 206 closure; Plan 202 omitted it.

## 13. Tests and static anti-cheat checks

Update `scripts/check-service-tunnel-acceptance-evidence.sh` so a Plan 206 pass requires source-level evidence that:

- the production manager delivery loop invokes the remote backend for a non-local peer;
- the backend invokes/reuses the existing destination/tunnel/router/Streaming layers;
- the qualification test connects to the manager's actual listener;
- the test does not count a separately constructed lower stack as manager delivery;
- Direction B exists and uses an independent client;
- remote success counters are not manually incremented in the external acceptance driver;
- the remote row cannot pass from `routing_decision_for()` alone.

Retain the Plan 202 negative/no-backend unit tests.

## 14. Expected files

Likely product files:

```text
crates/i2pr-daemon/src/service_delivery.rs
crates/i2pr-daemon/src/service_tunnels.rs
crates/i2pr-daemon/src/destination_tunnels.rs          # reuse first
crates/i2pr-daemon/src/destination_streaming.rs        # reuse first
crates/i2pr-daemon/src/router_i2np.rs                  # handle reuse only
crates/i2pr-daemon/src/inbound_dispatch.rs             # ownership hook if required
crates/i2pr-daemon/src/service_generation.rs           # generation-safe registrations if required
```

Likely qualification files:

```text
crates/i2pr-daemon/tests/service_tunnels_remote_transport_qualification.rs
tests/integration/service-tunnels/run-independent.sh
scripts/check-service-tunnel-acceptance-evidence.sh
.github/workflows/service-tunnels-external.yml
```

Avoid adding new protocol implementations if existing Plan 184–193 code can be reused.

## 15. Required validation

At minimum on the exact Plan 206 candidate head:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh

bash tests/integration/service-tunnels/run-independent.sh
bash tests/integration/m6-interop/run-m6-mixed-router.sh

cargo deny check advisories bans sources
```

The exact-pinned external lane must run both generic directions on the same head. If either direction is flaky, require two complete clean repetitions before promotion.

## 16. Acceptance criteria

Plan 206 passes only when all are true:

1. `ServiceDestinationDelivery` is an executable production backend, not only a marker/counter object.
2. One router-wide backend is shared; no per-service duplicate router stack exists.
3. `i2pr-service-tunnels` remains transport-agnostic.
4. Normal manager connection/delivery code calls the remote backend for non-local peers.
5. `RemoteRouter` is not returned merely because a marker object exists.
6. Remote resolution uses existing validated LS2/NetDB machinery.
7. Counted remote path uses real non-zero-hop i2pr tunnel material.
8. Outbound service Streaming requests traverse existing StreamingDestinationAdapter + Garlic/TunnelData + router delivery.
9. Inbound remote traffic reaches the owning committed service runtime.
10. ACK/retransmit/close traffic uses the remote backend consistently.
11. Lookup timeout is bounded and typed.
12. Tunnel loss/retry is bounded and typed.
13. Remote peer never silently falls back to local bridge.
14. Local co-owned tests remain green.
15. Plan 180 reconcile/generation semantics remain green.
16. Generic-client test starts at the actual service listener.
17. Generic-client small payload passes.
18. Generic-client multi-packet payload passes.
19. Generic-client reverse payload passes.
20. Generic-client half-close/EOF passes.
21. Generic-server Destination is remotely published and visible.
22. Independent i2pd client reaches the actual generic-server service path.
23. Generic-server target round-trip passes.
24. Remote counters are operation-derived, not manually synthesized by the acceptance driver.
25. `unknown_peer == 0` for reachable counted peers.
26. Local-coowned count is zero for counted remote peers.
27. Shutdown returns pending lookups/connections/backend registrations to baseline.
28. Exact i2pd pin is verified clean.
29. No public I2P, clearnet fallback, outproxy, private-state injection, fake LS2, or direct-destination SSU2 shortcut is used.
30. Existing M6 i2pd evidence remains green.

## 17. Authority transition

Before Plan 206 passes:

```text
plan_202 = partial-m10-remote-routing-capability-surface-superseded-by-plan206
m10_remote_transport_core = not-yet-passed
m10-remote-destination-streaming-composition = blocked/non-authoritative
```

On exact-head Plan 206 success:

```text
plan_206 = passed-m10-production-remote-delivery-composition-corrective
plan_202 = retained-partial-superseded-by-plan206
m10_remote_transport_core = passed-via-plan206
next_m10_application_plan = 207
```

Do not restore the HTTP/IRC rows here. Plan 207 owns application evidence.

## 18. Handoff

After Plan 206 success, execute Plan 207. Plan 205 may continue independently. Final Plan 204 convergence remains blocked until both the Java branch and Plan 207 are genuinely closed.
