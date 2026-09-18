# Plan 212 — M10 router-backed service-Destination material and canonical inbound Streaming corrective

Status: **registered executable corrective**.

This plan supersedes the over-strong product-closure interpretation of Plans 210 and 211 while retaining the useful work they landed. It is deliberately narrow: do **not** redesign Milestone 10 service profiles, do **not** add another transport/router stack, and do **not** weaken the retained local M10 evidence.

## 1. Objective

Close the remaining product defect that prevents Milestone 10 remote application traffic from using real I2P Destination network material end-to-end.

At Plan 212 completion, every counted remote M10 service flow must satisfy all of the following:

```text
local application socket
  -> existing ServiceTunnelManager profile loop
  -> existing service-owned canonical StreamingManager
  -> router-backed per-service Destination network state
       real installed outbound tunnel role
       real signed Standard LeaseSet2 from real inbound lease(s)
       real inbound receive tunnel id ownership
       ordinary remote LeaseSet2 lookup by the actual remote Destination hash
       canonical ECIES/Garlic session state
  -> existing RouterDeliveryService / authenticated SSU2
  -> independent i2pd service
```

and the return path must be:

```text
SSU2 I2NP
  -> real inbound TunnelData route
  -> receive TunnelId -> owning ServiceRuntime
  -> recovered Garlic
  -> DestinationDispatcher::dispatch_garlic_envelope
  -> dispatcher.pop_payload(local Destination)
  -> StreamingDestinationAdapter::receive
  -> the SAME canonical service StreamingManager
  -> existing service socket pump
  -> local application socket
```

No counted remote path may source its outbound role or local LeaseSet2 from `SamLocalProductFabric`.

## 2. Why this corrective is required

Source head audited before registration: `2d5408baaa25aec09c42ec9ebd719e4dcf302af8`.

Plan 210 landed several useful structural changes:

- receive-tunnel-id -> service runtime reverse ownership;
- removal of the explicit `dummy_outbound_tunnel()` swap inside remote composition;
- explicit remote Destination hash lookup input instead of `SHA256(router.info)`;
- recovered Garlic entering `DestinationDispatcher`;
- typed inbound/outbound remote counters.

Those are retained.

However, the actual service runtime is still created by:

```rust
let fabric = SamLocalProductFabric::new();
let product = fabric.prepare_for_destination(&identity, now_seconds)?;
```

in `crates/i2pr-daemon/src/service_tunnels.rs::create_bridge_for_spec`.

`crates/i2pr-daemon/src/sam/fabric.rs` explicitly defines that fabric as a localhost-only, authenticated-router-link-bypassed seam and generates synthetic tunnel material. Therefore the bridge field now described as the "real" outbound role is still synthetic for remote M10 use.

A second defect is that `ServiceProduct::dial_and_bootstrap` builds a real exploratory outbound/inbound pair, but removes the real outbound role from the registry only to drive lookup and does not install that role into a service Destination runtime. The real inbound receive ids are likewise not automatically registered against the owning service runtime.

A third defect is inbound completion. `SamDestinationBridge::dispatch_inbound_garlic_owned` currently authenticates/processes the Garlic envelope and records the `InboundDispatchOutcome`, but does not drain `DestinationDispatcher::pop_payload(...)` into `StreamingDestinationAdapter::receive(...)`. Therefore successful Garlic authentication is currently stronger than actual remote Streaming delivery.

A fourth defect is lookup ownership. `ReferencePeer` still represents one application `destination_hash`, while Plan 211 can configure at least two distinct independent remote Destinations (HTTP and IRC). Router bootstrap and application Destination lookup must be separate operations.

Plan 211's application harness is retained, but it remains blocked until this product seam is corrected.

## 3. Retained evidence and non-goals

Retain without rewriting:

- Plans 174–180 local service-tunnel profile behavior.
- Plan 182 local/co-owned delivery driver.
- Plan 193 exact-pinned i2pd lower-layer Streaming/tunnel/NetDB proof.
- Plan 208 local-miss -> remote backend call graph.
- Plan 210 receive-id owner table and explicit destination-hash lookup direction.
- Plan 211 real HTTP/IRC specs, i2pd public Destination extraction, curl/jaraco subprocesses, and command-derived subfact framework.
- Plan 181's retained 29 local rows.

This plan must **not**:

1. create a second `Ssu2DaemonService` per service;
2. create a second `DestinationTunnelCoordinator` per service;
3. create a second `ExploratoryBuildCoordinator` per service;
4. create a second `StreamingManager` for counted remote traffic;
5. move protocol logic into test-only code;
6. patch i2pd;
7. require public I2P network access;
8. change the Java M6 branch;
9. weaken the local `SamLocalProductFabric` path;
10. claim M10 final acceptance before Plan 211 requalification is green.

## 4. Architecture lock

### 4.1 One router-wide network stack

The daemon/product composition owns exactly one shared:

- `Ssu2DaemonService` / `Ssu2DaemonHandle`;
- `RouterDeliveryService`;
- `DestinationTunnelCoordinator`;
- `ExploratoryBuildCoordinator`;
- authoritative NetDB stores.

Service Destinations consume typed handles/material from that shared stack. They do not own transports or router-wide coordinators.

### 4.2 Two different concepts must remain separate

`SamLocalProductFabric` may remain as the already-proven **local/co-owned** service seam.

Remote-capable network state must be a distinct optional attachment to the same service Destination runtime. Do not silently relabel localhost fabric material as router-backed material.

Recommended shape in `crates/i2pr-daemon/src/sam/streams.rs`:

```rust
pub(crate) struct RouterDestinationNetworkState {
    routing: DestinationRouting,
    session_manager: EciesSessionManager,
    outbound_role: DestinationOutboundRole,
    lease_set2: LeaseSet2,
    validated_lease_set2: ValidatedLeaseSet2,
    inbound_receive_ids: Vec<TunnelId>,
}
```

Exact field layout may vary, but the invariants may not:

- `outbound_role` originates from an installed `ExploratoryBuildCoordinator` outbound role;
- `lease_set2` is signed by the service Destination identity from actual inbound lease metadata;
- `inbound_receive_ids` come from the installed real inbound tunnel registry;
- remote routing/session state is attached to the existing service bridge/runtime;
- the existing canonical service `StreamingManager` remains authoritative.

Do **not** create `RouterDestinationNetworkState` if an existing type already cleanly carries these fields. Reuse an existing type where possible.

### 4.3 Local/co-owned traffic remains local

The retained local path may continue to use `SamLocalProductFabric`.

Remote compose must explicitly select router-backed state. A missing router-backed state must return a typed failure (`NotInstalled` / `NoTunnelMaterial` or a new narrow equivalent). It must never fall back to the local fabric.

## 5. Phase A — correct current authority before implementation

The planning commit that registers Plan 212 must classify:

```text
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-blocked-by-plan212
plan_212 = registered-executable
m10_remote_transport_core = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

The retained 29 local Plan 181 rows remain passed.

## 6. Phase B — add an explicit router-backed material installation surface

Primary files:

- `crates/i2pr-daemon/src/sam/streams.rs`
- `crates/i2pr-daemon/src/service_tunnels.rs`

Add a narrow bridge/runtime API. Recommended names:

```rust
SamDestinationBridge::install_router_network_state(...)
SamDestinationBridge::clear_router_network_state(...)
SamDestinationBridge::has_router_network_state()
SamDestinationBridge::router_network_summary()
```

and manager wrappers if needed:

```rust
ServiceTunnelManager::install_service_router_material(service_id, material)
ServiceTunnelManager::clear_service_router_material(service_id)
```

Requirements:

1. Installation must be fail-closed if material belongs to a different Destination identity.
2. Re-install without explicit replacement must fail.
3. Installation must not alter the local/co-owned bridge material.
4. Material must carry actual expiry information.
5. Expired outbound/inbound material must not be used.
6. No secret material may be logged or exposed through diagnostics.
7. A read-only diagnostic summary may expose only service id, Destination hash, receive tunnel ids, expiry, and counts.

## 7. Phase C — restructure `ServiceProduct::start` ordering

Primary file:

- `crates/i2pr-daemon/src/service_product.rs`

Current ordering builds the exploratory pair before service runtimes exist. That prevents the real tunnel material from becoming service-Destination-owned material.

Change startup to this order:

```text
1. validate controlled SSU2 config
2. start shared SSU2 service
3. bootstrap/verify reference RouterInfo only
4. construct ServiceTunnelManager
5. manager.prepare() to create service identities/listeners WITHOUT starting supervisors
6. for every enabled remote-capable service runtime:
      build real outbound + inbound tunnel material
      derive the service's real local LS2
      install router-backed state on that exact service runtime
      register real inbound receive tunnel ownership
      resolve configured remote target LeaseSet2 (client profiles)
      publish local LS2 when server reachability requires it
7. only after all required services are network-ready:
      start service supervisors
8. fail atomically if any mandatory service provisioning fails
```

A failed network provisioning pass must not leave half-started application listeners accepting traffic.

If `prepare()` already binds listeners before supervisors, that is acceptable as long as no accept loop starts before network readiness and failure cleanup closes staged listeners.

## 8. Phase D — separate router bootstrap from application Destination lookup

`ReferencePeer` is router transport/bootstrap metadata. It must not represent one application Destination.

Remove application `destination_hash` from `ReferencePeer` after callers are migrated.

`dial_and_bootstrap(...)` must do only:

- RouterInfo verification;
- authoritative RouterInfo bootstrap;
- authenticated SSU2 dial/session establishment;
- peer material preparation required for real tunnel builds.

It must **not** perform an application LeaseSet2 lookup.

Add a separate bounded operation for each service target, e.g.:

```rust
resolve_remote_destination_for_service(
    service_runtime,
    destination_hash,
    ...
)
```

The remote Destination hash must derive from the actual `DestinationRef`:

- `Base32Hash` -> the configured 32-byte Destination hash;
- `ConfiguredDestination` -> decode Destination and hash it;
- `StaticAlias` -> resolve alias, then apply the above;
- local co-owned hash -> stay local and do not enter this path.

HTTP and IRC targets must be independently resolvable in the same product instance. Never assume the first target hash applies to all services.

## 9. Phase E — build real per-service Destination tunnel material

Use the exact lower-layer pattern already proven by `crates/i2pr-daemon/tests/destination_tunnel_external.rs` and `streaming_tunnel_external.rs` rather than creating a new tunnel representation.

### 9.1 Outbound

For each remote-capable service Destination:

1. issue a real outbound build through the shared `ExploratoryBuildCoordinator`;
2. wait for `BuildCoordinatorOutcome::Installed`;
3. obtain the installed `TunnelSlot`;
4. take the real gateway role using the existing registry path:

```rust
let gateway_role = coordinator
    .registry_mut()
    .remove_outbound(outbound_slot)
    .ok_or(...)?;

let outbound_role = DestinationOutboundRole::from_role(
    gateway_role,
    expiry_ms,
);
```

5. store that `DestinationOutboundRole` in the service's router-backed state.

Do not use `dummy_outbound_tunnel`, `random_outbound_tunnel`, `LocalZeroHop`, `SamLocalProductFabric`, or test fixtures for this state.

### 9.2 Inbound

Build a real inbound tunnel through the same shared coordinator and capture its installed registry metadata.

Construct one or more `InboundLeaseSource` values from the actual installed inbound route. Follow the Plan 187/193 pattern:

```rust
InboundLeaseSource::from_parts(
    slot,
    ibgw_router_hash,
    ibgw_receive_tunnel_id,
    expires,
    usable_until,
)
```

The values must come from installed route metadata, not hard-coded Plan 193 constants.

Build the service Destination's Standard LS2 with:

```rust
build_signed_lease_set2(&service_identity, &lease_sources, published)
```

Validate it through the existing `ValidatedLeaseSet2` path and install both plaintext + validated forms into router-backed service state.

### 9.3 Tunnel id allocation

The current controlled helper uses fixed constants suitable for one pair. Plan 212 must support multiple service Destinations in one process.

Use an existing tunnel-id allocator if one exists. Otherwise add one small private allocator in daemon composition with these rules:

- never emit zero;
- never reuse an id still present in the registry;
- allocate a disjoint set for every build request in the process;
- bounded/wrapping collision search;
- no global mutable static;
- deterministic unit tests are allowed with an injected seed/start value.

Do not add an external crate solely for id allocation.

## 10. Phase F — production inbound owner registration lifecycle

The Plan 210 owner table is retained but must become production-owned rather than test-only/aspirational.

When a real inbound tunnel is installed for a service:

```rust
manager.register_inbound_tunnel_owner(receive_id, Arc::clone(runtime))?
manager.register_inbound_destination_owner(destination_hash, Arc::clone(runtime)).await?
```

Register every local receive id that can deliver to that Destination.

On:

- tunnel expiry;
- tunnel replacement;
- service replacement;
- generation drain completion;
- product shutdown;

remove the corresponding mappings.

Acceptance invariant:

```text
inbound_tunnel_owner_pairs()
    == live router-backed inbound service routes
```

No synthetic localhost-fabric receive id may appear in this table for a counted remote path.

## 11. Phase G — use router-backed material for remote composition

Replace the current Plan 210 remote composition dependency on bridge-local fabric fields.

Current helper:

```rust
SamDestinationBridge::compose_adapter_send_owned_fields(...)
```

must either be renamed/reworked to operate explicitly on router-backed state, or a new method must be added, e.g.:

```rust
SamDestinationBridge::compose_router_send(...)
```

The call must pass the router-backed:

- `DestinationRouting`;
- `EciesSessionManager`;
- `DestinationOutboundRole`;
- real local `LeaseSet2`;
- same Destination identity;
- same queued `TransportSendRequest` from the canonical Streaming manager.

The remote send path must fail if router-backed state is missing/expired.

Static source checks must make it difficult to regress to the local fabric accidentally.

## 12. Phase H — ordinary remote LeaseSet2 lookup per service target

For each remote target hash needed by an enabled client profile:

1. compute the routing key using the existing NetDB helper;
2. select the service's real inbound reply route;
3. call `DestinationTunnelCoordinator::begin_lease_lookup`;
4. compose lookup via the service's real router-backed outbound role using `compose_lookup_via_tunnel`;
5. pump returned TunnelData through the existing `inbound_dispatch` path;
6. ingest `DatabaseStore` / `DatabaseSearchReply` through the coordinator;
7. require a validated cached LS2 for exactly the target hash;
8. install that validated LS2 into that service's router-backed `DestinationRouting`.

Do not pre-seed the counted target LS2 directly in the service routing table.

Cache sharing at the router-wide coordinator is allowed. Per-service routing installation is still required because each service owns independent ECIES/Streaming state.

## 13. Phase I — server-side local LS2 publication

Client-only HTTP/IRC/generic client Destinations do not need floodfill publication solely to receive replies when their real LS2 is carried in Streaming establishment.

Server profiles that independent routers must initiate toward (`GenericServer`, `IrcServer`, and any future remote server profile) do require ordinary publication.

For each server Destination:

1. use the real signed LS2 from Phase E;
2. invoke the existing `DestinationTunnelCoordinator` publication state machine;
3. compose the DatabaseStore through the service's real outbound tunnel;
4. require the existing ACK/publication success semantics;
5. retain/retry only through existing bounded coordinator policy.

Do not add a second publication implementation to service tunnels.

## 14. Phase J — finish inbound Garlic -> canonical Streaming

Primary files:

- `crates/i2pr-daemon/src/sam/streams.rs`
- `crates/i2pr-daemon/src/service_product.rs`
- optionally `crates/i2pr-daemon/src/service_tunnels.rs` for the manager wrapper/notification

The Plan 210 helper currently stops after `DestinationDispatcher::dispatch_garlic_envelope`.

Replace that boolean-only completion with an operation that delivers authenticated destination payloads to canonical Streaming.

Recommended shape:

```rust
pub(crate) struct RouterInboundDispatchReport {
    pub garlic_authenticated: bool,
    pub payloads_dequeued: usize,
    pub streaming_packets_accepted: usize,
    pub streaming_rejected: usize,
}
```

Exact type/name may vary.

Required sequence:

```text
1. decode recovered standard Garlic I2NP envelope
2. dispatch_garlic_envelope against router-backed ECIES/session/routing state
3. if rejected -> return typed rejected report
4. repeatedly call dispatcher.pop_payload(local_destination_id)
5. for every dequeued Data payload:
     determine the authenticated remote Destination hash/session peer
     StreamingDestinationAdapter::receive(
         queued.bytes(),
         &service_identity,
         &mut canonical_streaming_manager,
         remote_destination_hash,
         now_ms,
     )
6. preserve normal Streaming SYN/SYN-ACK/DATA/ACK/close processing
7. wake the existing destination delivery driver if receive processing queued an outbound response
8. advance remote_inbound_dispatched ONLY after >=1 Streaming payload was accepted
```

### 14.1 Remote peer hash correctness

Do not substitute the router hash for the remote Destination hash.

For a new inbound Streaming session, derive the remote Destination identity/hash from authenticated destination-layer information already returned/installed by the dispatcher (for example the validated remote LS2 key).

For subsequent packets, retain/use the peer Destination hash associated with the established destination/streaming session.

If the current API does not expose this cleanly, add the smallest typed association needed on the bridge. Do not use a process-global "current reference destination" variable.

### 14.2 One canonical Streaming manager

`StreamingDestinationAdapter::receive` must target the existing service-owned canonical `StreamingManager` used by the application socket pump.

Do not send remote inbound payloads to:

- a new `StreamingManager`;
- a Plan 210-only receiver mirror if the application loop does not read it;
- a test driver queue;
- an independent SAM runtime.

## 15. Phase K — I2NP decode parity

`ServiceProduct::process_inbound` currently uses short-transport-only decode at its outer boundary while other production/interop paths use standard-first / short fallback where required.

Audit the exact inbound SSU2 I2NP envelope contract and mirror the established production dispatcher ordering.

If the SSU2 runtime guarantees short transport here, document/assert that guarantee. If both forms are possible, use the same standard-first/short-fallback helper already used elsewhere.

Do not add a third independent decoder implementation.

## 16. Phase L — generic product qualification before HTTP/IRC

Add one ignored external qualification driver, recommended path:

```text
crates/i2pr-daemon/tests/service_tunnels_plan212_router_backed_product.rs
```

It must consume the same product composition API that Plan 211 consumes. The driver must not construct lower router/tunnel/Streaming objects.

Run against exact-pinned unmodified i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`) in the existing controlled interop environment with the required peer cache.

### Direction A — i2pr generic client -> i2pd STREAM service

Prove:

- real enabled `GenericClient` spec;
- independent i2pd SAM STREAM destination;
- ordinary target LS2 lookup;
- real service outbound role installed;
- real service local LS2 contains a real inbound lease;
- TCP client connects to the i2pr loopback listener;
- Streaming reaches `Established`;
- small payload digest round trip;
- multi-packet payload digest round trip;
- inbound counter advances only after adapter receive;
- local-coowned delivery delta remains zero;
- `unknown_peer` delta remains zero;
- resource baseline clean after close.

### Direction B — i2pd initiator -> i2pr generic server

Prove:

- real enabled `GenericServer` spec;
- stable service Destination identity;
- real local LS2 publication succeeds;
- i2pd resolves/uses that Destination;
- i2pd STREAM CONNECT reaches i2pr through real inbound tunnel;
- owner resolution occurs by the real receive TunnelId;
- SYN reaches canonical service Streaming listener;
- local fixture accepts connection;
- bidirectional payload digest equality;
- clean close/resource baseline.

Direction B must be mandatory for Plan 212 pass. If environment topology cannot execute it, Plan 212 remains blocked with exact provenance.

## 17. Phase M — unit/integration test matrix

Add focused tests for at least these conditions:

1. router-backed material installation accepts matching Destination identity;
2. mismatched Destination identity fails;
3. duplicate install fails unless explicit replace path is used;
4. clear removes router-backed state;
5. expired outbound role is rejected;
6. real-material compose path refuses missing router-backed state;
7. remote compose does not read `SamLocalProductFabric` material;
8. two services can hold distinct outbound roles simultaneously;
9. two services can hold distinct inbound receive ids simultaneously;
10. receive-id registration follows real installed material;
11. unregister on replacement removes stale receive ids;
12. router bootstrap no longer requires an application destination hash;
13. HTTP target and IRC target can resolve to two distinct hashes in one product;
14. lookup uses the actual service target hash;
15. server LS2 is built from real `InboundLeaseSource` values;
16. server publication cannot run with local-fabric leases;
17. Garlic auth without payload does not increment `remote_inbound_dispatched`;
18. Garlic with one accepted Streaming payload increments it exactly once;
19. multiple payload cloves are drained rather than only the first;
20. `StreamingDestinationAdapter::receive` operates on the canonical service Streaming manager;
21. queued SYN-ACK/ACK wakes the existing outbound delivery driver;
22. orphan receive id fails closed;
23. stale drained generation receive id fails closed;
24. local/co-owned Plan 182 path remains green;
25. retained local M10 application suites remain green.

## 18. Phase N — static checker hardening

Extend `scripts/check-service-tunnel-acceptance-evidence.sh` with Plan 212 invariants.

Required positive checks:

- router-backed network state type/field exists on the service bridge/runtime;
- production provisioning calls `register_inbound_tunnel_owner`;
- production provisioning calls real build coordinator paths;
- remote compose calls the router-backed compose method;
- inbound router dispatch contains `pop_payload`;
- inbound router dispatch contains `StreamingDestinationAdapter::receive`;
- Plan 212 external driver exists and is `#[ignore]` gated;
- Plan 212 external driver does not import forbidden lower stack types.

Required negative checks:

- `compose_remote_cells` / router compose must not call `SamLocalProductFabric`;
- router compose must not call `dummy_outbound_tunnel`;
- router compose must not use `random_outbound_tunnel` / localhost fabric lease material;
- `ReferencePeer` must not carry an application `destination_hash` after migration;
- remote inbound success must not be recorded immediately after `dispatch_garlic_envelope` without adapter receive;
- counted driver must not call `record_observation` / `record_remote_application_observation`;
- counted driver must not directly construct `StreamingManager`, `StreamingDestinationAdapter`, `DestinationTunnelCoordinator`, `ExploratoryBuildCoordinator`, `Ssu2DaemonService`, or `RouterDeliveryService`.

Keep the checker structural; do not make it parse Rust semantically.

## 19. Phase O — validation floor

Before external qualification:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo doc --locked --workspace --no-deps
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
```

Do not spend time fixing unrelated pre-existing warnings unless the Plan 212 changes caused them or CI treats them as fatal.

## 20. Phase P — Plan 211 requalification after Plan 212

Only after the Plan 212 generic Direction A + Direction B product lane is green should the implementation agent rerun the existing Plan 211 application lane.

Do not rewrite Plan 211's curl/jaraco evidence framework unless Plan 212 API changes require a mechanical adaptation.

Expected sequence:

```text
Plan 212 generic product A+B green
  -> Plan 211 HTTP remote row green
  -> Plan 211 IRC remote row green
  -> retained 29 local rows still green
  -> exact-head external repeat run green
  -> M10 may close
```

If HTTP/IRC fails after generic A+B succeeds, classify the failure at the application/profile boundary and do not reopen the lower router stack without evidence.

## 21. Evidence requirements

Plan 212 evidence must record only public/sanitized facts.

Required external evidence keys should include equivalents of:

```text
plan212-i2pd-pin-ok
plan212-router-bootstrap-ok
plan212-service-destination-hash
plan212-real-outbound-installed
plan212-real-inbound-installed
plan212-local-ls2-real-lease-count
plan212-inbound-owner-registered
plan212-remote-ls2-lookup-started
plan212-remote-ls2-lookup-succeeded
plan212-direction-a-stream-established
plan212-direction-a-small-digest-match
plan212-direction-a-large-digest-match
plan212-direction-a-inbound-streaming-accepted
plan212-direction-b-local-ls2-published
plan212-direction-b-inbound-owner-hit
plan212-direction-b-stream-established
plan212-direction-b-small-digest-match
plan212-direction-b-large-digest-match
plan212-orphan-receive-delta-zero
plan212-local-coowned-delta-zero
plan212-unknown-peer-delta-zero
plan212-resource-baseline-clean
```

Never record private Destination keys, tunnel layer keys, SSU2 static private keys, decrypted Garlic plaintext, or application payload contents.

## 22. Stop conditions

Stop and leave Plan 212 blocked if any of these are true:

1. the only way to make remote compose work is to feed it `SamLocalProductFabric` tunnel material;
2. the implementation requires a second router-wide SSU2/NetDB/tunnel stack;
3. the implementation requires a second canonical Streaming manager;
4. real inbound owner registration cannot be tied to installed receive tunnel metadata;
5. the remote service LS2 cannot be built from real inbound lease metadata;
6. the external lane cannot distinguish router-backed from local-fabric material;
7. Direction A or Direction B only passes after manually incrementing counters;
8. i2pd must be patched;
9. public I2P access becomes mandatory;
10. retained local M10 behavior regresses and the regression is not a direct, understood consequence of the new router-backed state split.

On a stop condition, record the exact boundary and create a narrower successor plan. Do not broaden Plan 212 into a new M6/M10 umbrella.

## 23. Explicit acceptance criteria

Plan 212 is passed only if all are true on the implementation head:

1. `SamLocalProductFabric` remains explicitly local/co-owned only.
2. A service Destination can own optional router-backed network state.
3. Router-backed state contains a real installed outbound role.
4. Router-backed state contains a real signed LS2 built from real inbound lease metadata.
5. Router-backed state contains real inbound receive tunnel ids.
6. Service identity matches the LS2 and network state identity.
7. Missing router-backed state fails closed for remote compose.
8. Expired router-backed state fails closed.
9. `ServiceProduct::start` prepares identities before per-service network provisioning.
10. Supervisors do not begin accepting application traffic before mandatory network provisioning succeeds.
11. Router bootstrap is independent of application Destination lookup.
12. `ReferencePeer` no longer represents one application Destination.
13. Two distinct remote service target hashes can coexist in one product instance.
14. Remote LS2 lookup is keyed by the actual target Destination hash.
15. Lookup uses the service's real outbound role and real inbound reply route.
16. Validated remote LS2 installs into that service's router-backed routing state.
17. Production code automatically registers inbound receive-id ownership.
18. Drain/replacement/shutdown remove stale ownership.
19. Server Destinations publish their real LS2 through the existing publication state machine.
20. Client-only Destinations are not forced to publish merely to receive replies.
21. Recovered Garlic is authenticated through the router-backed destination session state.
22. Destination payloads are drained with `pop_payload`.
23. Every accepted remote Streaming payload enters `StreamingDestinationAdapter::receive`.
24. Adapter receive targets the same canonical service `StreamingManager` used by the application pump.
25. `remote_inbound_dispatched` advances only after actual Streaming payload acceptance.
26. Outbound response work queued by receive wakes/reaches the existing delivery driver.
27. Generic Direction A passes against exact-pinned unmodified i2pd 2.61.0.
28. Generic Direction B passes against exact-pinned unmodified i2pd 2.61.0.
29. Direction A small + multi-packet digests match.
30. Direction B small + multi-packet digests match.
31. Counted remote path does not construct/use a shadow lower stack.
32. Counted remote path does not use local-fabric tunnel material.
33. Retained local 29-row M10 matrix remains green.
34. Routine CI/static/dependency/documentation floors are green.
35. Plan 211 remote HTTP and IRC rows are rerun after Plan 212 and either both pass or remain honestly blocked with exact provenance.
36. No status/README/AGENTS file claims M10 closed unless the Plan 211 exact-head external row set is genuinely green.

## 24. Recommended implementation order for a smaller model

Use this order. Do not begin with the Plan 211 driver.

```text
Commit A:
  router-backed state type + bridge install/clear/accessors
  unit tests for identity/expiry/duplicate behavior

Commit B:
  ServiceProduct startup reorder
  router bootstrap decoupled from app destination hash
  per-service real outbound/inbound provisioning
  production owner registration

Commit C:
  per-service target LS2 lookup
  server local LS2 publication
  multi-service target tests

Commit D:
  inbound Garlic -> pop_payload -> StreamingDestinationAdapter::receive
  canonical manager + wakeup/counter semantics
  focused unit/live tests

Commit E:
  Plan 212 ignored generic Direction A/B driver
  runner integration + static checker
  execute external lane

Commit F only after E is green:
  mechanical Plan 211 adaptation if required
  rerun HTTP/IRC exact-head lane
  authority normalization
```

If one commit becomes too large, split within the phase, but do not reorder the dependency chain.

## 25. Success authority transition

Only after Plan 212 generic A+B passes may status move to:

```text
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-ready-for-requalification
plan_212 = passed-m10-router-backed-service-destination-and-canonical-inbound-streaming
m10_remote_transport_core = passed-via-plan212
```

Only after the subsequent Plan 211 exact-head HTTP + IRC lane passes may M10 move to:

```text
plan_211 = passed-m10-product-only-remote-http-and-irc-application-closure
m10_remote_application_interop = passed-via-plan211-after-plan212
milestone10_remote_service_interop = passed-via-plan212-and-plan211
milestone10_final_acceptance = closed-via-plan211-after-plan212
next_product_layer = milestone11-planning
```

The Java M6 second-family branch remains independent. Later Plan 204 may normalize cross-milestone authority after both branches are separately closed.
