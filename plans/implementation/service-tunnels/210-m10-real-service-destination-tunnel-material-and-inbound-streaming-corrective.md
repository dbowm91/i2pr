# Plan 210 — M10 real service-Destination tunnel material and inbound Streaming integration corrective

Status: **registered / executable**.

Source floor: `16f569a1b3ef57310f038d0776b95ac0d2a4ad9d`.

Supersedes for execution: the over-promoted remote-transport interpretation of Plans 208/209. Retain their useful code and historical evidence; do not delete them.

## 1. Objective

Close the remaining M10 product-boundary defect: a normal `ServiceTunnelManager` service Destination must use real router-owned destination tunnel material for remote traffic, and recovered remote Garlic traffic must terminate in the actual service-owned `StreamingManager`.

This plan is product code, not an evidence-only pass.

The required end state is:

```text
local TCP / HTTP / SOCKS / IRC profile
  -> service-owned StreamingManager
  -> normal ServiceTunnelManager delivery driver
  -> destination-aware LeaseSet2 resolution
  -> StreamingDestinationAdapter
  -> service Destination ECIES/Garlic state
  -> REAL service Destination outbound tunnel
  -> RouterDeliveryService / authenticated SSU2

remote TunnelData
  -> existing tunnel reassembly
  -> recovered Garlic I2NP
  -> inbound tunnel-id ownership lookup
  -> owning service DestinationDispatcher / ECIES session
  -> StreamingDestinationAdapter::receive
  -> SAME service-owned StreamingManager
  -> existing socket/Streaming pump
  -> local application socket
```

Plan 210 does **not** reimplement Streaming, ECIES, Garlic, tunnel building, NetDB, SSU2, HTTP, SOCKS5, or IRC.

## 2. Why this corrective exists

The source floor has two useful but incomplete changes:

1. Plan 208 wired the normal `ServiceTunnelManager::deliver_outbound` local-miss branch into `route_outbound_remote_request`, so non-co-owned traffic can reach the remote backend.
2. Plan 209 moved lower-stack construction out of the counted application driver into `ServiceProduct::start`.

The remaining product defects are narrower but fundamental:

- service runtimes are still prepared through `SamLocalProductFabric`;
- the remote compose path still sources the local LeaseSet2 and `DestinationOutboundRole` from the local SAM bridge, including the `dummy_outbound_tunnel()` swap placeholder;
- `ServiceProduct::start` builds a router-level exploratory inbound/outbound pair but does not install real destination tunnel material for each service Destination;
- `ServiceProduct` derives a LeaseSet2 lookup target as `SHA256(reference.router_info_bytes)`, which is a RouterInfo-derived value, not the actual service Destination hash;
- `ServiceProduct::poll_inbound` drains remote `TunnelData`, but recovered `Garlic` is not actually decrypted/dispatched into the owning service Destination Streaming runtime;
- the existing `destination_hash -> ServiceRuntime` owner table is insufficient to select the owner before Garlic decryption, because raw inbound tunnel traffic is first identified by the receiving tunnel route;
- server-side independent initiation requires the service Destination's real LeaseSet2 to be reachable; client-only service destinations need a real reply LeaseSet2 but need not be floodfill-published merely to originate a stream.

Do not close these gaps by adding test-only injection, fake leases, direct destination-over-SSU2, or another parallel Streaming/router stack.

## 3. Retained architecture and non-negotiable invariants

Retain all of the following:

- one router-wide authenticated SSU2/runtime owner;
- one shared `RemoteDestinationBackend` / authoritative `DestinationTunnelCoordinator`;
- `ServiceTunnelManager` remains the M10 application composition root;
- each service Destination retains its own identity and Streaming/session/routing state;
- the Plan 182 local/co-owned bridge remains the local fast path;
- `StreamingDestinationAdapter` remains the only Streaming-to-destination-routing composition seam;
- direct destination-over-SSU2 remains prohibited for counted remote paths;
- private Destination keys never enter evidence/logging;
- `i2pr-service-tunnels` remains runtime/transport neutral;
- existing resource ceilings and transactional generation semantics remain intact.

The remote path must not use:

- `SamLocalProductFabric` synthetic tunnel material;
- `LocalZeroHop` material;
- `dummy_outbound_tunnel()` as a counted remote role;
- a preloaded/fake LeaseSet2 as a substitute for ordinary lookup;
- driver-owned `StreamingManager`, `DestinationRouting`, `EciesSessionManager`, `DestinationTunnelCoordinator`, `ExploratoryBuildCoordinator`, `Ssu2DaemonService`, or `RouterDeliveryService`.

## 4. Required design: separate local bridge state from real remote network material

Do not destroy the existing local bridge; it is valid for local/co-owned M10 rows.

Introduce a daemon-owned per-service remote-network material surface. Suggested shape (names may differ if a cleaner existing abstraction is available):

```rust
struct ServiceDestinationNetworkMaterial {
    destination_id: DestinationId,
    lease_set2: LeaseSet2,
    outbound_role: DestinationOutboundRole,
    inbound_receive_tunnels: Vec<TunnelId>,
    published: bool,
}
```

Ownership requirements:

- secret tunnel role material has one owner;
- no cloning or reconstructing layer keys;
- the map is keyed by the actual service `DestinationId`;
- the material is installed only after a real inbound and outbound destination route are usable;
- every inbound receive tunnel id is registered in a reverse owner map;
- expiration/failure/drain removes the material and reverse routes atomically;
- local/co-owned delivery continues to use the existing local bridge without consulting this map.

Recommended manager state:

```text
remote_destination_material: DestinationId -> ServiceDestinationNetworkMaterial
inbound_tunnel_owners: receive TunnelId -> { DestinationId, generation/runtime }
inbound_hash_owners: DestinationHash -> runtime   // retained post-decrypt sanity/index
```

If borrow ownership makes a `DestinationOutboundRole` awkward to hold beside bridge session/routing state, add a narrow composition method that borrows the real role and the bridge state together. Do **not** temporarily replace it with `dummy_outbound_tunnel()` in the remote path.

## 5. Phase A — stage identities before starting supervisors

Current service runtime construction already creates/loads the service Destination identity. Reuse that identity; do not create a second remote-only identity.

Refactor `ServiceProduct::start` / manager preparation order so product composition can provision remote material before traffic is accepted:

```text
start daemon-owned SSU2/router substrate
 -> build ServiceTunnelManager
 -> manager.prepare() / stage service identities + listeners, supervisors not running
 -> provision real destination tunnel material for remote-capable services
 -> install service remote material + inbound ownership
 -> publish server LeaseSets where required
 -> start supervisors/listeners accepting application traffic
```

If `prepare()` currently starts a listener bind, that is acceptable; it must not begin accepting/counting traffic until remote material is ready.

Failure anywhere before commit must leave the prior generation intact and must not leave orphan tunnel ownership mappings.

## 6. Phase B — real destination tunnel provisioning

Reuse the existing tunnel build machinery proven by Plans 185/193. Do not build another short-record or crypto implementation.

For every remote-capable service Destination that requires dedicated material:

1. obtain the service's existing `DestinationIdentity`;
2. allocate unique bounded creator/receive tunnel ids; remove Plan 209's single fixed service-agnostic tunnel-id assumption;
3. build at least one real outbound and one real inbound destination tunnel using the existing coordinator/build path;
4. require successful install into the existing data-plane registry;
5. obtain the real outbound gateway role by move from the registry/owner;
6. derive one or more `InboundLeaseSource` records from the real inbound registration;
7. build the signed Standard LeaseSet2 with `build_signed_lease_set2` using the **service identity** and those real inbound leases;
8. call the existing remote-material verifier or equivalent and reject zero-hop/synthetic material;
9. install the resulting material in the manager's per-service network-material map;
10. register every local receive tunnel id in `inbound_tunnel_owners`.

`DestinationPolicy::Dedicated` must get dedicated material. `SharedClientGroup` may share one explicitly named client Destination/material set only where the existing M10 policy already permits it. Never make sharing implicit as an optimization.

The implementation may introduce a small allocator/helper for tunnel ids, but it must remain bounded by configured service/destination limits.

## 7. Phase C — remove RouterInfo-derived remote Destination lookup

`ServiceProduct` must stop doing:

```text
DestinationHash(SHA256(reference.router_info_bytes))
```

for service LeaseSet2 lookup.

The reference RouterInfo is transport/bootstrap material only. Remote service lookup must key on the actual remote **Destination**.

Required behavior:

- `DestinationRef::ConfiguredDestination` -> decode public Destination -> actual Destination hash + public descriptor;
- `DestinationRef::Base32Hash` -> use decoded destination hash; obtain any additional public Destination/session material from the validated remote LeaseSet2 or an existing typed conversion path;
- `DestinationRef::StaticAlias` -> resolve strictly through the configured alias table, then follow one of the above paths;
- no DNS/clearnet fallback;
- no RouterInfo hash substitution.

Move remote LeaseSet2 resolution from router bootstrap into a destination-aware operation driven by the actual service connect target.

Suggested daemon API:

```text
ensure_remote_lease_set2(
    service_destination_id,
    remote_destination_hash,
    deadline
) -> validated cached LeaseSet2 / typed pending/failure
```

It must reuse:

- authoritative `DestinationTunnelCoordinator`;
- ordinary floodfill candidate selection;
- the service Destination's real outbound route;
- a real inbound reply path registered for that service/router composition;
- existing LS2 validation/cache;
- bounded retries/deadlines.

## 8. Phase D — lookup-pending behavior must preserve the real Streaming request

A cache miss on the first SYN must not become `unknown_peer` or silently drop the request.

Choose one existing bounded mechanism and make it authoritative:

- preferably the existing destination-routing pending-outbound queue, if it already preserves `TransportSendRequest` correctly; or
- a manager-level bounded pending-remote queue keyed by `(service DestinationId, remote DestinationHash)`.

Required semantics:

```text
queued Streaming request arrives
 -> remote LS2 cached? compose/send immediately
 -> not cached? start/coalesce bounded ordinary lookup
 -> keep the SAME request pending
 -> validated LS2 arrives
 -> install into SAME service bridge routing state
 -> retry SAME queued request through normal adapter
 -> terminal lookup failure -> fail/close connection with typed reason
```

No unbounded polling. No duplicate lookup storm for the same remote. No application payload in diagnostics.

## 9. Phase E — remote outbound compose must use real service material

Update `compose_remote_cells` (or its successor) so the remote branch uses:

- actual service identity;
- same service `DestinationRouting`;
- same service `EciesSessionManager`;
- real service `DestinationOutboundRole` from Plan 210 material;
- real signed service LeaseSet2 built from real inbound leases;
- existing `StreamingDestinationAdapter::send`;
- existing `deliver_outbound_cells`;
- shared `RouterDeliveryService`.

Remove the counted remote dependency on:

```text
bridge.outbound_role() from SamLocalProductFabric
bridge.lease_set2() from SamLocalProductFabric
dummy_outbound_tunnel() placeholder swap
```

The same `TransportSendRequest` drained from the service-owned Streaming manager must be the one composed and sent.

Advance `remote_outbound_composed` only after the real adapter composition succeeds. Advance the outbound delivery/request counter only after accepted router delivery.

## 10. Phase F — inbound tunnel ownership before Garlic decryption

Add an authoritative reverse route:

```text
local receive TunnelId -> owning service DestinationId/runtime/generation
```

Register it when a real inbound service tunnel becomes active. Remove it on expiration, failure, replacement, or generation drain.

`ServiceProduct::poll_inbound` must preserve the inbound receive-tunnel context from `dispatch_inbound_tunnel_data` or the data-plane registry. If the current `InboundDispatchOutcome` loses the local receive-id after reassembly, extend the typed outcome narrowly so the caller can recover the route owner without parsing payload content.

Do not guess ownership from remote peer/router identity.

Unknown/stale inbound tunnel ids fail closed and advance a bounded diagnostic rejection counter.

## 11. Phase G — finish Garlic -> service Streaming dispatch

The current Plan 209 comment saying Garlic is consumed internally is not sufficient. Implement the actual production call chain.

For one recovered Garlic envelope:

1. resolve owning service from inbound tunnel id;
2. get the exact service bridge/runtime for the committed generation;
3. decode standard I2NP Garlic using existing codecs;
4. invoke that service's existing `DestinationDispatcher::dispatch_garlic_envelope` with the same service `EciesSessionManager`, service identity keys, time, and validated LS2 store;
5. pop the recovered payload for that service Destination;
6. invoke `StreamingDestinationAdapter::receive` with the exact service identity and the same service-owned canonical `StreamingManager` used by HTTP/IRC/generic/SOCKS;
7. let normal Streaming processing enqueue SYN-ACK/ACK/data/close responses;
8. notify/wake the same normal delivery driver so those responses leave through the Plan 210 real remote path;
9. only then advance `remote_inbound_dispatched`.

Do not create a receiver-mirror `StreamingManager` for counted remote traffic. If the legacy local bridge needs its receiver mirror for local/co-owned routing, retain it only on that local branch.

## 12. Phase H — client vs server LeaseSet publication

Client-only services:

- require real inbound tunnel(s) and a signed current local LeaseSet2 so a fresh New Session can carry valid reply information;
- do not require floodfill publication solely to originate a connection;
- may remain unpublished unless another existing policy explicitly requests publication.

Server services (`generic-server`, `irc-server`):

- must publish their current real signed LeaseSet2 through the existing `DestinationTunnelCoordinator::begin_ls2_publication` / tunnel publication path before external initiation is counted;
- publication must use real service outbound material and a real selected floodfill;
- require protocol-derived acknowledgment where the existing publication state machine requires it;
- republish on lease rotation before the safety margin expires;
- never publish a zero-hop/synthetic LeaseSet.

Publication state belongs to the product lifecycle, not the acceptance driver.

## 13. Phase I — lifecycle, rotation, and reconcile correctness

Wire remote material into existing generation semantics.

On prepare/commit:

- stage material before supervisors accept traffic;
- do not expose half-provisioned service Destinations.

On reconcile:

- unchanged identity/config may retain usable material where safe;
- replacement identity must receive fresh real material before commit;
- install new inbound routes before removing old only where generation ownership is unambiguous;
- draining generation may finish existing connections within existing deadlines;
- no new connections may route to a drained generation.

On tunnel expiry/failure:

- remove invalid role/lease sources;
- mark service remote path unavailable;
- schedule bounded replacement through existing build policy;
- never fall back to local fake material.

On shutdown:

- stop listeners/supervisors;
- clear pending LS2 resolutions;
- unregister inbound route/hash ownership;
- drop/zeroize secret material through existing owners;
- return active service connections/destination routes to baseline.

## 14. Focused tests required before external execution

Add deterministic tests covering at least:

1. remote service material rejects `LocalZeroHop`;
2. remote service material rejects dummy/synthetic role installation;
3. real material is keyed to the correct `DestinationId`;
4. two dedicated services cannot alias inbound receive tunnel ids;
5. explicit shared-client group sharing remains bounded/intentional;
6. RouterInfo bytes are never used as the service Destination lookup key;
7. Base32 target lookup uses the decoded Destination hash;
8. configured public Destination target lookup uses `Destination::hash()`;
9. cache miss starts/coalesces bounded lookup;
10. original `TransportSendRequest` remains pending across lookup;
11. lookup success retries the original request;
12. lookup terminal failure closes/fails without local fallback;
13. remote compose reads Plan 210 real outbound role + LS2;
14. counted remote compose never calls `dummy_outbound_tunnel()`;
15. inbound receive tunnel id resolves the correct service runtime;
16. stale/unknown inbound tunnel fails closed;
17. recovered Garlic dispatch reaches that service dispatcher;
18. protocol-6 payload reaches the same canonical service Streaming manager;
19. inbound Streaming response wakes normal outbound driver;
20. client service LS2 is usable without mandatory floodfill publication;
21. server service publication uses real material;
22. generation replacement removes old inbound ownership;
23. shutdown clears remote-material/owner/pending-resolution state;
24. retained local/co-owned bridge tests remain unchanged/green.

## 15. Generic external product qualification gate

Before Plan 211, add or replace the current Plan 208 remote qualification with a **product-only generic service** external lane against exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`).

The counted driver may use only:

- `ServiceProduct` public product API;
- `ServiceTunnelManager` product-facing snapshot/listener information exposed through `ServiceProduct`;
- OS TCP sockets/standard generic byte client;
- reference i2pd public SAM/tunnel configuration/setup from the shell harness.

It may not construct lower-stack i2pr components.

Direction A — i2pr client service -> i2pd server Destination:

- actual local generic client TCP listener;
- independently created i2pd server Destination;
- small payload exact digest;
- >= 32 KiB multi-packet payload exact digest;
- reverse response exact digest;
- orderly EOF/close;
- positive LS2 lookup evidence for the actual remote Destination;
- positive real outbound compose/delivery delta;
- positive inbound dispatch delta;
- `local_coowned_deliveries` delta = 0;
- `unknown_peer` delta = 0.

Direction B — i2pd client -> i2pr generic server Destination:

- actual i2pr generic-server Destination with published real LS2;
- actual loopback TCP fixture target;
- i2pd independently resolves/connects to the i2pr public Destination;
- fixture receives exact payload;
- reverse bytes return to i2pd;
- positive inbound dispatch + outbound response deltas;
- no local/co-owned bridge counted;
- clean shutdown baseline.

Plan 210 does not pass if only Direction A is green.

## 16. Evidence integrity and static checker changes

Extend `scripts/check-service-tunnel-acceptance-evidence.sh` to reject at least:

- `SHA256(reference.router_info_bytes)` as a service LeaseSet lookup target;
- remote compose use of `dummy_outbound_tunnel()`;
- counted remote use of `SamLocalProductFabric` tunnel material;
- Plan 210 external driver construction of lower-stack i2pr types;
- literal/manual success increments for `remote_outbound_composed` or `remote_inbound_dispatched`;
- missing inbound-tunnel owner registration in production lifecycle;
- recovered `I2npBody::Garlic` branch that returns/ignores without product dispatch;
- server external row without real LS2 publication evidence;
- external generic Direction B implemented through a private injection/API instead of independent i2pd initiation.

Keep the existing Plan 207/209 anti-shadow checks where they still add value.

## 17. Full validation floor

Run on the implementation head:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

Then run the Plan 210 generic external lane on the same exact head.

## 18. Explicit acceptance criteria

Plan 210 passes only when all are true:

1. One router-wide SSU2/router backend remains authoritative.
2. Each counted dedicated service Destination uses real non-zero-hop inbound/outbound tunnel material.
3. No counted remote path uses `SamLocalProductFabric` tunnel material.
4. No counted remote path uses `dummy_outbound_tunnel()`.
5. The service identity used by the application profile is the identity whose LS2/tunnel material is used remotely.
6. Local signed LS2 leases correspond to real registered inbound tunnel routes.
7. Remote service LS2 lookup is keyed by the actual remote Destination hash.
8. RouterInfo-byte hashing is absent from service Destination lookup.
9. Remote lookup is ordinary, bounded, validated, and cached through the existing coordinator.
10. Cache miss preserves the original queued Streaming request until success/failure.
11. Lookup success retries that request through the normal driver.
12. Outbound compose uses the same service-owned Streaming/session/routing state.
13. Outbound compose uses the real service Destination outbound role.
14. Outbound New Session carries the real current local service LS2 where required by the existing adapter/session contract.
15. Inbound local receive tunnel ids map to the correct owning service generation.
16. Recovered Garlic is actually decrypted/dispatched by the owning service dispatcher.
17. Protocol-6 payload reaches the same canonical service-owned `StreamingManager` used by the application socket pump.
18. Streaming-generated responses return through the same normal Plan 210 delivery driver.
19. `remote_inbound_dispatched` advances only after successful service Streaming dispatch.
20. `remote_outbound_composed` advances only after real adapter composition.
21. Client-only services have valid reply LS2/tunnels without requiring fake publication.
22. Server services publish real LS2 before independent inbound initiation is counted.
23. Generation replacement/drain updates inbound ownership without stale cross-generation delivery.
24. Tunnel expiry/loss fails closed and never falls back to local synthetic material.
25. Direction A generic external product qualification passes against exact-pinned unmodified i2pd.
26. Direction B generic external product qualification passes against exact-pinned unmodified i2pd.
27. Both directions have command/digest/counter-delta/cleanup evidence from the same run.
28. The counted external driver constructs no lower-stack i2pr networking object.
29. Retained local 29-row M10 evidence remains green.
30. Full routine validation floor is green on the exact implementation head.

## 19. Stop conditions

Stop and record a typed blocker instead of weakening the plan if:

- existing tunnel-build APIs cannot transfer/install real material for a service Destination without violating ownership;
- the inbound data-plane outcome cannot preserve the local receive tunnel identity needed for owner routing;
- Standard LeaseSet2 decoding lacks enough public Destination material to satisfy Base32-only remote descriptor construction;
- server LeaseSet publication cannot be performed through the existing coordinator without a concrete protocol defect;
- exact-pinned i2pd rejects product traffic after Plan 193-equivalent wire bytes are proven.

Any such blocker gets a narrow follow-up plan. Do not create a broad new harness.

## 20. Terminal authority transition

On success update status authority to:

```text
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = passed-m10-real-service-destination-network-material-and-inbound-streaming
m10_remote_transport_core = passed-via-plan210
m10_generic_remote_product = passed-bidirectional-via-plan210
next_m10_plan = 211
```

Do **not** claim HTTP/IRC remote application closure here. That belongs to Plan 211.
