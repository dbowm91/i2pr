# Plan 232 status — M6 Java route-derived lease-gateway fixture corrective and second-family closure

Status: **registered-ready-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure**.

Plan of record:
[`232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure.md`](../../implementation/mixed-router-interop/232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure.md).

## Registration basis

Plan 231 closed as:

```text
passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
```

Its deepest counted run proved Java A outbound-gateway enqueue, Router C exact
one-hop OBEP processing, and exact absence of the selected target IBGW on
Router B, with zero matching i2pr TunnelData.

Current driver source proves the fixture mismatch:

```text
installed inbound route gateway = service_hash (Router A)
installed gateway tunnel        = IBGW_RECEIVE

published local LS2 gateway     = java_hash (Router B)
published local LS2 tunnel      = IBGW_RECEIVE
```

The same hard-coded `java_hash` lease-gateway assumption exists at exactly
three current local-lease construction sites in
`crates/i2pr-daemon/tests/java_tunnel_external.rs`:

1. raw-Destination local LS2;
2. initial Streaming local LS2;
3. refreshed/republication Streaming local LS2.

Plan 232 corrects the fixture by deriving lease gateway + gateway tunnel from
the installed inbound route, while preserving Router B as the independent
NetDB publication target.

## Current authority

```text
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
plan_232 = registered-ready-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure

plan_201 = blocked-pending-plan232-route-derived-lease-gateway-fixture-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan232
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## Authorized work

- derive all Java-driver local LS2 lease gateway/tunnel fields from the installed
  inbound route;
- assert route ↔ lease parity before publication;
- retain Router B as the independent publication/floodfill target;
- rerun raw-Destination qualification;
- if reverse raw delivery passes, continue directly through existing Java
  Streaming qualification;
- close Java second-family M6 directly if all retained mandatory rows pass.

## Prohibited shortcuts

No production protocol change under this fixture corrective; no hard-coded
Router-A replacement as the new source of truth; no publication-target
redirection; no topology/profile/tunnel/NetDB policy changes; no timeout
inflation; no Java source patching/reflection/private mutation; no public I2P,
reseed, VMComm, or `netDb.alwaysQuery`.

Closure must replace this registration token with the exact executed result and
update Plan 201, Plan 204, registry, and M6/M10 roadmap projections together.
