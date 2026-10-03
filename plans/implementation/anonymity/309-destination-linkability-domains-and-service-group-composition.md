# Plan 309 — Destination linkability domains and service-group composition

Status at registration: **blocked-on-plan307**

Classification: production architecture + capability.

Hard dependencies: Plan 307 passed; ADR 0030. Retained Plan 305 source audit is input evidence.

## 1. Objective

Replace the service-owned Destination assumption with an explicit Destination-group owner so users may configure multiple service instances, intentionally share one Destination across client services, and expose multiple server services on one persistent Destination using distinct I2P ports.

This plan establishes ownership and composition only. Real multi-hop pool selection is Plan 310.

## 2. Why ready after Plan 307

Plan 305 localized the owner mismatch but over-constrained sharing. ADR 0030 resolves the architecture: the group is the linkability domain and the router must stay outside it.

## 3. Current implementation evidence

`ServiceRuntime` currently contains one `destination_id`. `DestinationPolicy` models `Dedicated` and `SharedClientGroup`, but current composition does not implement a general shared owner and server sharing is rejected. Exact-pinned i2pd demonstrates shared local Destinations for HTTP/SOCKS and configured client/server tunnels, with server dispatch distinguished by Destination and inbound port.

## 4. Invariants

1. Destination group, not service kind or remote target, is the explicit linkability domain.
2. Multiple instances of any service kind are valid within resource ceilings.
3. `Dedicated` remains shorthand for an implicit unique group.
4. Explicit sharing never occurs as fallback.
5. Client-only groups are ephemeral by default.
6. Any group containing a server is persistent.
7. Mixing client and server services in one explicit persistent group is allowed and documented as intentional linkage.
8. Router identity/keys are never reused as group identity.
9. One group has one authoritative identity/pool/lifecycle owner; no duplicate registry stack.
10. Server inbound I2P ports are unique within a group.

## 5. Scope

Introduce `DestinationGroupId` and group configuration/ownership; migrate `ServiceRuntime` to reference a group handle; group persistent storage; service-to-group membership validation; server port demultiplexing; client cross-kind sharing; legacy `Dedicated` and `SharedClientGroup` compatibility; multiple same-kind service tests.

Explicitly out: peer selection, multi-hop path building, lifecycle timing smoothing, Streaming tuning, and target-isolated mode.

## 6. Required production changes

1. Add runtime-neutral `DestinationGroupSpec/Id` and explicit service group reference.
2. Preserve `Dedicated` as implicit unique-group shorthand.
3. Map legacy `SharedClientGroup` to the general group model or reject only genuinely ambiguous forms.
4. Build one group registry in `ServiceTunnelManager`; services hold a group handle rather than owning identity state directly.
5. Persist identity by group id for groups containing server services; keep client-only explicit groups ephemeral unless future config says otherwise.
6. Move inbound/outbound owner indexes, delivery-driver association, publication context, and Destination-registry ownership to the group.
7. Route server inbound connections by group Destination plus I2P port and reject duplicate group/port registrations.
8. Allow HTTP, SOCKS, IRC, and generic client services to share an explicit group.
9. Permit multiple same-kind service definitions up to existing/configured hard resource ceilings; no kind-singleton maps.
10. Emit a local warning when a persistent group contains client services because that activity is intentionally linkable to the published server Destination.

## 7. Ordered work packages

WP1 config/types and migration; WP2 group registry/lifecycle skeleton; WP3 identity/storage migration; WP4 service membership/client sharing; WP5 server-port demux; WP6 reconcile/drain ownership; WP7 compatibility/regression tests.

## 8. Failure, cancellation, restart, and contention

Group creation is transactional. Concurrent services referencing one group coalesce on one owner. Partial creation rolls back indexes/storage handles. Generation drain marks a group draining only when no committed service generation still references it. Persistent group keys survive restart; ephemeral group keys do not.

## 9. Compatibility and migration

Existing configs using `Dedicated` retain behavior. Existing serialized server keys migrate from service-owned lookup to a deterministic group storage namespace without changing key bytes. Legacy `SharedClientGroup` is accepted as an alias where unambiguous.

## 10. Required tests

Multiple HTTP proxies; multiple SOCKS proxies; HTTP+IRC same group; HTTP+SOCKS same group; two dedicated HTTP services get distinct Destinations; same explicit group gets one Destination; server services on two distinct synthetic ports share one Destination; duplicate group/port rejected; persistent mixed-group restart; client-only group restart rotates; reconcile retains/removes groups correctly; resource ceilings fail closed.

## 11. Exact verification commands

Full workspace floor plus focused `i2pr-service-tunnels`, `i2pr-daemon`, `i2pr-storage`, and `i2pr-client` tests and all existing service-boundary scripts.

## 12. Documentation updates

Document Destination groups as intentional linkability domains and show examples for shared HTTP+SOCKS, multiple same-kind client proxies, and multiple server services on one Destination.

## 13. Acceptance criteria

No service kind is singleton; explicit group sharing works across client kinds; server-port multiplexing works; dedicated shorthand remains; group identity storage/restart semantics are correct; router identity is never used; full test/security floor green.

## 14. Stop conditions

Stop if implementation requires a second Destination registry, incompatible persistent-key rewrite, or implicit sharing under capacity pressure.

## 15. Closure evidence required

Config matrix, lifecycle state diagram, persistence migration tests, same-kind/group/server-port fixtures, exact command results.

## 16. Handoff

On pass, Plan 310 becomes ready. Plan 308 may already be proceeding independently.
