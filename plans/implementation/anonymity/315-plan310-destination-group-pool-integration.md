# Plan 315 — Plan 310 corrective: Destination-group pool ownership and Destination-operation integration

Status at registration: **blocked-on-plan314**

Classification: production architecture + anonymity capability.

Hard dependencies: Plan 314 passed; Plan 309 passed; ADR 0030.

## 1. Objective

Complete the remaining blocked Plan 310 requirements by making each explicit Destination group own the real multi-hop `DestinationTunnelPool` built from Plan 314 material, then make LeaseSet generation, remote lookup/publication, and service data use that group-owned pool.

This plan is the consumer/integration half of the Plan 310 corrective. It does not redo peer selection or short-build cryptography.

## 2. Why this is a separate corrective

Plan 310 combined two ownership changes that are independently substantial:

1. candidate selection plus multi-hop build submission; and
2. group-owned pool lifecycle plus every Destination consumer moving onto that pool.

Plan 314 isolates and proves the first boundary. Plan 315 can therefore operate on typed established material instead of simultaneously redesigning selection, building, pool ownership, and service routing.

This split follows the planning rule that repeated corrective work should revise milestone sizing rather than repeat an oversized pass.

## 3. Existing implementation evidence

The implementation agent must re-confirm:

- Plan 309 gives every explicit group one `DestinationGroupRuntime`, identity, bridge, Destination id, persistence policy, and membership set.
- `i2pr-client::DestinationTunnelPool` already owns bounded inbound/outbound registrations, failure thresholds, deterministic expiry, usability checks, and `inbound_lease_sources`.
- the current service product provisions once per distinct Destination id but takes material out of the process-wide `ExploratoryBuildCoordinator`;
- `RouterDestinationNetworkState` currently stores exactly one `DestinationOutboundRole`, one generated LeaseSet2, a vector of receive ids, and aggregate expiries;
- service lookup/publication/send paths borrow that single router-backed outbound role;
- the current LeaseSet is built from one selected inbound route rather than the authoritative group pool.

These are the remaining Plan 310 ownership mismatches.

## 4. Invariants

1. One explicit Destination group owns one bounded `DestinationTunnelPool`; services in that group do not own independent network pools.
2. Distinct groups never share a pool merely because of capacity pressure.
3. Only Plan 314-qualified complete paths may enter a remote group pool.
4. No LocalZeroHop/fabric/exploratory role may satisfy the remote anonymity-qualified group pool.
5. LeaseSet leases derive only from currently usable inbound entries in the group pool.
6. Remote service data, LeaseSet lookup, and publication use outbound material selected from that same group pool.
7. Expiry/removal immediately prevents stale material from being advertised or selected.
8. Replacement is bounded by existing Destination policy/failure thresholds and global build-concurrency limits.
9. Scarcity/failure never falls back to direct transport, exploratory tunnels, another group's pool, or a shorter path.
10. Group identity and pool lifecycle are independent of router identity.
11. Plan 311, not this plan, owns startup timing smoothing and graceful-retirement timing.

## 5. Target architecture

~~~text
DestinationGroupRuntime
  identity
  bridge / Streaming state
  DestinationTunnelPool  <--- authoritative network pool
  bounded build/replenishment state
        ^
        |
        | EstablishedMaterial from Plan 314
        |
Destination multi-hop build owner

Consumers of the SAME pool:
  inbound leases -> signed group LeaseSet2
  outbound tunnel -> service data
  outbound tunnel -> remote LeaseSet lookup
  outbound tunnel -> LeaseSet publication

service A ----+
service B ----+--> group runtime/pool
service C ----+
~~~

There must not be a second pool hidden in `RouterDestinationNetworkState` or the service product.

## 6. Required production changes

### WP1 — Put the canonical pool under Destination-group ownership

Extend `DestinationGroupRuntime` (or an immediately-owned group network-state object) with exactly one `DestinationTunnelPool` configured from the group's `DestinationConfig`.

All members referencing the same group must observe the same pool identity and counts. Dedicated services continue to behave as one-member groups.

Do not place the pool on individual `ServiceRuntime` values.

### WP2 — Register Plan 314 established material directly into the group pool

The Plan 314 Destination build owner must hand successful `EstablishedMaterial` to the owning group.

Register inbound/outbound material through `DestinationTunnelPool::register_inbound` / `register_outbound` or a narrow extension of those canonical APIs.

Preserve:

- direction checks;
- pool capacity;
- configured tunnel lifetime;
- failure threshold;
- zeroization/drop semantics;
- exact three-hop established topology.

A build that succeeds cryptographically but cannot register due to pool capacity or invalid material must return a typed group-build outcome and release the material.

### WP3 — Add bounded normal replenish/expiry/replacement

Implement the minimum normal active-state lifecycle needed before Plan 311:

- compute deficits against configured inbound/outbound targets;
- submit bounded replacement builds through the Plan 314 owner;
- never exceed global pending-build/resource ceilings;
- advance deterministic pool time and evict expired entries;
- remove failed entries and account the existing failure threshold;
- stop replacement when the threshold pauses the pool;
- coalesce duplicate replenish triggers for one group/generation;
- cancel pending group builds when the group generation is removed/reconciled.

Do not implement startup delay, one-second staging, graceful-retirement windows, or eleven-minute shutdown behavior here; those belong to Plan 311.

### WP4 — Make the group pool the LeaseSet source

Build/refresh the group's signed Standard LeaseSet2 from `DestinationTunnelPool::inbound_lease_sources(now)`.

Requirements:

- every advertised lease maps to a currently usable registered inbound tunnel;
- expired/failed entries disappear before signing/refresh;
- lease count follows pool state and configured publication policy;
- the Destination identity signing the LeaseSet is the group's identity;
- no router hash or exploratory route is substituted for missing group leases.

A group without the required usable inbound minimum is not publishable.

### WP5 — Replace single-outbound-role router state with group-pool selection

The current `RouterDestinationNetworkState.outbound_role` is a one-tunnel snapshot and cannot be the final group owner.

Refactor router-backed Destination state so outbound composition borrows/selects a usable outbound path from the canonical group pool. The exact internal API may be a group pool handle plus a narrow role projection, but there must be one authoritative set of registrations.

Update all remote consumers:

- Streaming/service outbound data;
- `DestinationTunnelCoordinator::compose_lookup_via_tunnel`;
- LeaseSet publication composition;
- reply-path/inbound ownership registration where applicable.

Selection among multiple usable outbound tunnels must be bounded and must not expose peer identities in diagnostics. Preserve any existing correctness requirement that a specific operation retain one chosen tunnel for its bounded lifetime rather than changing path mid-message.

### WP6 — Make inbound ownership group-derived

Register every usable inbound receive id from the group pool with the existing inbound owner/dispatch machinery.

When a pool entry expires or fails, remove its corresponding receive ownership before it can be treated as live. Multiple services in one group must dispatch through the one group Destination identity/bridge.

Tests must prove there is no per-service duplicate owner for shared groups.

### WP7 — Remove/adapt legacy per-service provisioning

Replace `provision_all_service_router_material`'s one-pass per-runtime/single-peer assumptions with group-oriented provisioning.

The resulting composition should iterate distinct Destination groups, not services, for:

- pool creation;
- initial builds;
- inbound owner registration;
- LeaseSet construction;
- remote lookup setup;
- server publication.

Shared service members may still have independent listeners/backends/ports, but not independent Destination network pools.

Do not retain the old one-peer provisioning path as a silent fallback.

## 7. Failure, cancellation, restart, and contention

Group creation/provisioning is transactional: a group does not become remotely usable until its minimum required pool state and signed LeaseSet state are valid.

Concurrent replenish triggers coalesce per group/generation. Build permits are released on success, rejection, timeout, cancellation, group removal, and pool-registration failure.

A daemon restart reconstructs ephemeral group pools; persistent server-group identity keys remain unchanged. No tunnel cryptographic material is persisted.

If a group loses below-minimum pool capacity during active operation, new operations fail/queue only within existing bounded semantics; they must not borrow router/exploratory/other-group material.

Plan 311 will later define the state transition into `Retiring`; this plan only needs clean immediate generation cancellation/removal semantics.

## 8. Compatibility and migration

No user configuration migration is required.

Plan 309 Destination-group identity/persistence semantics remain unchanged. Server groups retain their key bytes across restart; client-only group identities remain ephemeral.

The internal `RouterDestinationNetworkState` shape may change substantially because its single outbound role is not compatible with authoritative group pools. Keep public/local service APIs stable where possible.

No new dependency is expected.

## 9. Required deterministic evidence

External multi-router execution is not a Plan 315 gate. Counted evidence must prove that the production group owner consumes the Plan 314 established material and every relevant Destination operation uses it.

Required rows include:

- one group receives exact three-hop inbound/outbound established material;
- two services in one explicit group observe one pool;
- two groups observe distinct pools;
- configured inbound/outbound targets replenish without exceeding ceilings;
- expiry removes material and triggers bounded deficit handling;
- failed build increments existing failure accounting;
- threshold pauses replacement;
- group removal cancels pending builds and releases pool material;
- signed LeaseSet contains leases derived from all/only usable group inbound entries;
- expired/failed inbound entries are absent from refreshed LeaseSet;
- lookup composition uses a group outbound path;
- publication composition uses a group outbound path;
- Streaming/service send uses a group outbound path;
- inbound receive ownership corresponds exactly to live group inbound entries;
- direct/exploratory/other-group fallback count remains zero;
- shared services do not duplicate publication or inbound ownership.

Use deterministic/manual time for expiry/replenishment tests.

## 10. Required tests

Add focused tests in the owning crates/modules plus integration tests covering:

- group pool creation once per Destination id/group;
- established material registration direction/capacity;
- three-hop topology retained after pool registration;
- pool target/deficit calculation;
- bounded replenish coalescing;
- cancellation and permit release;
- expiration and failure removal;
- failure-threshold pause/reset;
- LeaseSet derivation from pool;
- publication withheld below usable inbound minimum;
- multiple inbound leases when multiple usable inbound tunnels exist;
- outbound selection from canonical pool;
- lookup/publication/data-plane consumers use that selection;
- receive-owner add/remove with pool lifecycle;
- shared member behavior;
- distinct-group isolation;
- restart identity/pool behavior;
- no LocalZeroHop/exploratory/direct fallback for counted remote group operations.

## 11. Exact verification commands

Run, at minimum:

~~~text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-service-anonymity-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
git diff --check
~~~

No external i2pd lane is required for Plan 315 closure.

## 12. Documentation updates

Update service/anonymity architecture documents so they show:

- one group identity;
- one group `DestinationTunnelPool`;
- Plan 314 selector/build owner feeding that pool;
- LeaseSet/data/lookup/publication consuming that pool;
- exploratory/router pools as separate owners.

Remove prose that describes router-backed service state as one outbound role plus one inbound route if that is no longer true.

Document that deterministic multi-hop architecture evidence is distinct from later live multi-router interoperability.

## 13. Acceptance criteria

Plan 315 passes only if all are true:

1. every Destination group owns exactly one authoritative bounded `DestinationTunnelPool`;
2. only Plan 314-qualified established material enters remote group pools;
3. normal active-state replenish/expiry/failure handling is bounded and cancellation-safe;
4. LeaseSets derive solely from usable group inbound pool entries;
5. service data, remote lookup, and publication use usable group outbound pool material;
6. inbound owner registration tracks live group inbound entries;
7. shared services consume one group pool and distinct groups remain isolated;
8. no exploratory/direct/other-group/shorter-path fallback exists;
9. Plan 309 identity/persistence behavior remains correct;
10. required deterministic/workspace/security tests pass.

Passing Plan 315 satisfies the production multi-hop/group-pool requirements that blocked Plan 310. The historical Plan 310 closure record remains unchanged.

## 14. Stop conditions

Stop and write a closure record if:

- the existing `DestinationTunnelPool` cannot be the canonical owner without duplicating established tunnel state elsewhere;
- a consumer requires moving/duplicating secret established material instead of borrowing through a narrow owner API;
- inbound ownership cannot be removed consistently with pool expiry/failure;
- group pooling requires lifecycle timing semantics that belong to Plan 311;
- a required operation can function only by retaining an exploratory/direct fallback.

## 15. Closure evidence required

Record:

- group/pool ownership diagram;
- exact established-material handoff from Plan 314;
- pool target/replenish/expiry/failure matrix;
- LeaseSet derivation evidence;
- lookup/publication/service-send path evidence;
- inbound ownership lifecycle evidence;
- shared-vs-distinct group matrix;
- zero-fallback evidence;
- restart/persistence regressions;
- exact verification commands/results;
- security/compatibility findings;
- unblock audit for Plans 311 and 312.

## 16. Handoff

On pass, Plans 311 and 312 become dependency-ready in parallel.

Plan 313 remains blocked on Plan 312. Plan 308 remains independent.
