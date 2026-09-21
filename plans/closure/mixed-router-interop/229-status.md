# Plan 229 status — M6 Java non-zero exploratory paired-tunnel bootstrap corrective

Status: **`registered-ready-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective`**.

Plan of record:
[`229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective.md`](../../implementation/mixed-router-interop/229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective.md).

## Registration basis

Plan 228 closed with:

```text
P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both
```

on all four counted attempts.

The exact-pinned Java build path shows why this blocks the helper: stock client
tunnel construction requires an opposite-direction paired tunnel, and Java
rejects zero-hop exploratory fallback tunnels as paired infrastructure when
the exploratory pool is configured for non-zero operation.

Additional pinned-source and repo review found two concrete test-topology
issues that are sufficient to justify a corrective rather than another broad
diagnostic:

1. Java I2P ships a supported one-hop exploratory configuration in
   `installer/resources/small/router.config`:
   `router.inboundPool.length=1`,
   `router.inboundPool.lengthVariance=1`,
   `router.outboundPool.length=1`,
   `router.outboundPool.lengthVariance=1`.
2. The existing Plan-201 bootstrap says Router C is a non-floodfill tunnel
   participant, but the current launcher unconditionally sets
   `router.floodfillParticipant=true` for every A/B/C router. Pinned
   exploratory selection intentionally suppresses floodfills most of the time,
   which is undesirable in the tiny controlled peer population.

The harness already exchanges A/B/C RouterInfos through ordinary authenticated
I2NP DatabaseStore over real SSU2. That stock wire path remains the sole
profile-learning mechanism; Plan 229 does not authorize direct Java NetDB or
profile mutation.

## Current authority

```text
plan_228 = passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary
plan_229 = registered-ready-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective

plan_201 = blocked-pending-plan229-nonzero-exploratory-paired-tunnel-bootstrap-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan229
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

## Authorized correction

Plan 229 may change only the controlled Java test topology:

- explicit A/B/C role startup configuration;
- Router C to non-floodfill transit role;
- Router A only to the pinned upstream small-router exploratory profile;
- read-only profile/exploratory/client-build diagnostics;
- harness classification and evidence guards.

It then reruns the unchanged Plan-227 raw client profile and reuses Plan-228
build-path attribution.

## Prohibited shortcuts

Plan 229 does not authorize:

- production Rust behavior changes;
- Java source patches;
- direct profile creation/tier/score mutation;
- direct Java NetDB store/publish from launcher/probe;
- exploratory `explicitPeers`;
- direct tunnel installation;
- VMComm;
- `netDb.alwaysQuery`;
- public I2P/reseed;
- Plan-226 distinct topology;
- tunnel timeout changes;
- Plan-227 client-profile changes;
- Streaming-helper execution;
- target lookup/reverse-delivery qualification.

Closure must replace this registration token with exactly one Plan-229
terminal and update the unblock audit.
