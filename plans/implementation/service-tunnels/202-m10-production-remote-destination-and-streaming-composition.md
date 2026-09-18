# Plan 202 — M10 production remote Destination and Streaming composition

Status at registration: **registered-executable**.

Plan 202 is the product-integration pass exposed by the blocked Plan 199 execution. It is deliberately independent of the Java publication branch and may execute in parallel with Plan 200.

Plan 193 already proved the lower mixed-router destination/Streaming machinery against exact-pinned i2pd. Plan 182 proved the M10 service-tunnel manager can move bytes through a **local co-owned destination bridge**. What is missing is the production composition that connects those two layers.

## 1. Goal

Make a normal `ServiceTunnelManager` client/server destination capable of using the router's real remote path:

```text
loopback application socket
 -> M10 service profile
 -> per-service StreamingManager
 -> remote Destination resolution / Standard LS2
 -> real destination tunnel pool
 -> StreamingDestinationAdapter
 -> Garlic / TunnelData
 -> authenticated router transport
 -> independent i2pd router/service
```

and the reverse inbound path:

```text
independent router/service
 -> real i2pr inbound tunnel
 -> TunnelData recovery
 -> destination Garlic dispatch
 -> StreamingManager
 -> M10 application socket
```

The existing local co-owned bridge remains a valid optimization/test path but must no longer be the only delivery backend.

## 2. Dependency and parallelism

Plan 202 may start immediately from the Plan 199 blocked head.

It does **not** depend on Plan 200/201 because exact-pinned i2pd 2.61.0 already provides a retained-passed M6 family.

```text
plan200 || plan202 may execute in parallel
plan203 depends on plan202
plan204 depends on plan201 + plan203
```

## 3. Current architecture fact to preserve

At the Plan 199 head, `ServiceTunnelManager` owns:

- per-service `DestinationRuntime` / `StreamingManager` state;
- `SamDestinationHandle` and `SamDestinations` for local/co-owned delivery;
- per-destination local-delivery driver tasks;
- HTTP/SOCKS5/IRC profile supervisors;
- generation/reconcile/connection accounting.

Its Plan 182 delivery driver drains outbound Streaming requests through the SAM/local-product bridge. A structurally valid non-local destination increments `unknown_peer` and cannot route.

Separately, daemon modules already provide:

- `DestinationTunnelCoordinator` for authoritative RouterInfo/LeaseSet2 lookup/publication;
- real outbound/inbound tunnel registry/material;
- `StreamingDestinationAdapter`;
- `Ssu2DaemonService` / router delivery;
- `outbound_lookup` and `inbound_dispatch` tunnel composition;
- Plan 190 reply-path derivation;
- Plan 192 inbound destination wire compatibility.

Plan 202 must compose these existing layers. It must not reimplement them inside `i2pr-service-tunnels`.

## 4. Architectural boundary

The runtime-neutral `i2pr-service-tunnels` crate remains transport-agnostic.

Transport/tunnel/NetDB ownership stays in `i2pr-daemon`.

Preferred implementation shape:

```text
ServiceTunnelManager
  -> daemon-owned ServiceDestinationDelivery backend
       LocalCoOwned backend (existing Plan 182 seam)
       RemoteRouter backend (new Plan 202 composition)
```

A trait is not mandatory if a small enum/owned handle is clearer. Do not introduce a generic plugin architecture solely for this plan.

The important contract is that `ServiceTunnelManager` asks a daemon-owned delivery component to route queued Streaming requests; the manager does not create its own parallel router stack.

## 5. Phase A — locate and reuse the long-lived router owners

Before code changes, identify the daemon's existing owners/handles for:

```text
authenticated SSU2 router delivery
exploratory/destination tunnel registry
RouterInfo/LeaseSet2 authoritative stores
DestinationTunnelCoordinator state
inbound I2NP/TunnelData dispatch
cancellation/service graph
```

If these pieces currently exist only as individually composable production modules without one long-lived owner, Plan 202 may add one **shared daemon service/context** that owns them.

It must be:

- one router-wide owner, not one per M10 service;
- bounded;
- supervised through the existing service graph/child scope;
- reusable by future SAM/I2CP/service consumers;
- free of HTTP/IRC-specific behavior.

Do not instantiate a fresh `Ssu2DaemonService` per connection or per service tunnel.

## 6. Phase B — typed remote destination resolution

For a client tunnel whose destination is not co-owned locally:

1. parse/validate the configured Destination using existing codecs;
2. derive the `DestinationHash`;
3. consult the existing validated LeaseSet2 cache;
4. if absent/stale, issue a bounded ordinary lookup through `DestinationTunnelCoordinator`;
5. derive the reply path from a real registered inbound tunnel using Plan 190's adapter;
6. emit the DatabaseLookup through a real outbound tunnel;
7. recover and validate the returned Standard LS2;
8. construct the `RemoteDestination` / routing metadata required by the existing Streaming layer;
9. cache only through existing authoritative validated stores.

No DNS, outproxy, clearnet fallback, or direct-destination-over-SSU2 path is allowed.

## 7. Phase C — per-service real tunnel material

Each service Destination used for a counted remote connection must have usable real destination tunnel material.

Plan 202 may reuse the existing daemon pool/build machinery but must establish clear ownership/lifecycle for:

```text
inbound destination lease source(s)
outbound destination route
local Standard LS2 publication when the service must receive replies
liveness/rebuild on tunnel loss
shutdown/reconcile cleanup
```

Counted remote rows must reject `LocalZeroHop` material on the i2pr side.

The existing local bridge may still be selected for a destination that is truly co-owned by the same manager/generation.

## 8. Phase D — route outbound Streaming requests remotely

Replace the Plan 182 assumption:

```text
queued TransportSendRequest -> co-owned bridge only
```

with:

```text
queued TransportSendRequest
 -> if peer is co-owned: existing local bridge
 -> else: validated remote LS2 + destination tunnel pool
          -> StreamingDestinationAdapter::send
          -> compose outbound Garlic/TunnelData
          -> real router delivery service
```

Required properties:

- bounded queueing;
- typed timeout/cancellation;
- no silent local fallback for a remote destination;
- no direct SSU2 destination delivery counted;
- retransmits/ACKs/timers use the same remote route;
- route/tunnel loss returns a typed failure and triggers bounded recovery rather than spinning.

## 9. Phase E — route inbound remote destination traffic to the service runtime

The router-wide inbound path must deliver completed destination Garlic payloads to the owning M10 destination runtime.

Required chain:

```text
TunnelData on registered inbound tunnel
 -> recover Garlic bytes
 -> destination ECIES/Garlic dispatch
 -> owner DestinationId
 -> StreamingDestinationAdapter::receive
 -> owning StreamingManager
 -> queued ACK/data/control responses
 -> same delivery backend
```

Ownership lookup must be bounded and generation-safe. Draining/replaced service generations must not receive traffic for a newly committed identity.

## 10. Phase F — local path compatibility

Retain Plan 182 local round-trip behavior.

The routing decision must be explicit:

```text
LocalCoOwned => existing local bridge
RemoteResolved => real router/tunnel path
RemoteUnresolved => bounded lookup / typed connect failure
```

Do not remove the local seam merely to force every unit test through SSU2.

## 11. Phase G — first positive independent-router product test

Replace the current remote qualification's final expected outcome (`unknown_peer > 0`, `delivered == 0`) with a new positive core transport test against exact-pinned i2pd 2.61.0.

This Plan 202 test is **generic Streaming/product composition**, not yet HTTP/IRC acceptance.

Required Direction A:

```text
ordinary loopback TCP client
 -> i2pr generic-client service tunnel
 -> real remote Destination/Streaming path
 -> exact-pinned i2pd-owned destination
 -> independent loopback echo fixture
```

Required evidence:

- independent Destination public material generated by i2pd/public SAM or stock server tunnel facilities;
- real remote LS2 lookup;
- Streaming established;
- small payload digest equality;
- payload larger than one Streaming packet digest equality;
- reverse response digest equality;
- half-close/EOF behavior;
- clean shutdown.

Required Direction B core row:

```text
i2pd/public independent client
 -> published i2pr generic-server Destination
 -> real i2pr inbound tunnel/Streaming accept
 -> loopback target
 -> reply back to independent client
```

This proves the service destination publication/inbound side is not still local-only.

## 12. Suggested file ownership

Expected files to modify/add include:

```text
crates/i2pr-daemon/src/service_tunnels.rs
crates/i2pr-daemon/src/destination_tunnels.rs              # reuse first; modify only if a general seam is missing
crates/i2pr-daemon/src/router_i2np.rs                      # reuse delivery handle; avoid M10-specific protocol branches
crates/i2pr-daemon/src/inbound_dispatch.rs                 # reuse
crates/i2pr-daemon/src/outbound_lookup.rs                  # reuse
crates/i2pr-daemon/src/service_generation.rs               # only if generation ownership needs typed remote state
crates/i2pr-daemon/src/<new small remote delivery module>  # preferred if service_tunnels.rs would become unwieldy
crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs
crates/i2pr-daemon/tests/<positive remote transport test>.rs
tests/integration/service-tunnels/run-independent.sh       # core transport setup only; Plan 203 owns app promotion
```

Do not put SSU2/I2NP/NetDB code in `crates/i2pr-service-tunnels/`.

## 13. Counters and observability

Add typed privacy-safe counters sufficient to distinguish:

```text
remote_lookup_started
remote_lookup_succeeded
remote_lookup_failed
remote_stream_connect_started
remote_stream_established
remote_outbound_requests
remote_inbound_payloads
remote_route_retries
remote_route_timeouts
remote_tunnel_loss
local_coowned_deliveries
unknown_peer
```

`unknown_peer` must no longer be the expected success condition for a valid reachable independent destination.

Do not log payloads, private destination keys, Garlic session secrets, or raw LS2 private material.

## 14. Regression/hardening requirements

Preserve:

- Plan 180 transactional reconcile semantics;
- resource ceilings;
- listener bind rollback;
- persistent server destination identity;
- local round-trip tests;
- HTTP/SOCKS5/IRC policy behavior;
- cancellation and draining-generation cleanup;
- no clearnet fallback.

Add tests for:

```text
remote lookup timeout is bounded
remote tunnel loss fails/retries boundedly
remote destination malformed -> typed reject
stale LS2 -> lookup refresh
local co-owned peer still uses local path
remote peer never falls back to local bridge
reconcile cancels old remote driver state
shutdown leaves no router/service child tasks
```

## 15. Validation

Minimum Plan 202 floor:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash tests/integration/m6-interop/run-m6-mixed-router.sh
bash tests/integration/service-tunnels/run-independent.sh
cargo deny check advisories bans sources
```

The M10 runner may still report the final HTTP/IRC remote rows blocked until Plan 203. Plan 202 closes when the **generic remote transport** rows are positively command-derived.

## 16. Acceptance criteria

Plan 202 passes only when:

1. No per-service duplicate router/SSU2 stack is introduced.
2. Runtime-neutral service-tunnel crate remains transport-agnostic.
3. One shared daemon-owned remote delivery context exists.
4. Valid non-local Destination triggers real LS2 resolution instead of `unknown_peer` terminal failure.
5. LS2 resolution uses ordinary NetDB/tunnel machinery.
6. Counted i2pr remote path uses real non-zero-hop tunnel material.
7. Outbound Streaming requests traverse `StreamingDestinationAdapter` + real tunnel/router delivery.
8. Inbound remote Garlic/Streaming reaches the owning service destination.
9. Retransmit/ACK/timer traffic uses the same remote backend.
10. Tunnel loss/lookup timeout is bounded and typed.
11. Local co-owned delivery remains green.
12. Remote destinations never silently fall back to the local bridge.
13. Exact-pinned i2pd generic-client Direction A establishes.
14. Small payload digest matches.
15. Multi-packet payload digest matches.
16. Reverse payload digest matches.
17. Half-close/EOF is correct.
18. Exact-pinned independent client -> i2pr generic-server Direction B establishes.
19. Server loopback target round-trip passes.
20. Resource/task baseline returns clean.
21. Existing local M10 tests remain green.
22. Existing M6 i2pd rows remain green.
23. No public network/outproxy/private state shortcut is used.

## 17. Handoff

On success:

```text
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
m10_remote_transport_core = passed-via-plan202
next_m10_application_plan = 203
```

Plan 203 owns real curl/jaraco remote application acceptance. Plan 204 owns final milestone authority normalization.
