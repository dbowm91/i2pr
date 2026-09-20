# Plan 226 status — M6 Java loopback peer-diversity corrective

Status: **`registered-ready-m6-java-loopback-peer-diversity-corrective`**.

Plan of record:
[`226-m6-java-loopback-peer-diversity-corrective.md`](../../implementation/mixed-router-interop/226-m6-java-loopback-peer-diversity-corrective.md).

## Registration basis

Plan 225 closed with exact attribution:

```text
P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B
```

Retained facts:

- Router B has a current, validated, `receivedAsPublished=true`,
  query-answerable target Standard LS2;
- Router A's helper client DB lacks the target before/after send;
- helper lookup starts and fails;
- exact target lookup is not dispatched to B;
- Plan-222/223 exact selector includes B;
- reverse tracked send remains `ACCEPTED (1) -> NO_LEASESET (21)`;
- reverse payload remains absent in the frozen 45-second window.

Exact-pinned Java source review now identifies a narrow controlled-topology
hypothesis: `IterativeSearchJob` applies `IP_CLOSE_BYTES=3` through
`MaskedIPSet`, while the current harness advertises A/B/C SSU2 transports
from the same `127.0.0.0/24`.

Plan 226 must first prove the exact target-job Router-B
`Skipping query w/ router too close to others` branch. Only after that proof
may it place the three Java SSU2 RouterInfo hosts in distinct loopback /24s.

## Current authority

```text
plan_224 = passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap
plan_225 = passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution
plan_226 = registered-ready-m6-java-loopback-peer-diversity-corrective

plan_201 = blocked-pending-plan226-loopback-peer-diversity-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan226
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 226-m6-java-loopback-peer-diversity-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

## Registration constraints

Plan 226 is permitted to change only controlled Java test topology after exact
IP-diversity attribution.

It does not authorize:

- i2pr production protocol changes;
- Java source patches;
- `netDb.alwaysQuery` acceptance override;
- public/LAN routing;
- search-limit or tunnel tuning;
- publication retries;
- timeout changes.

Default corrective hosts in the plan are:

```text
A 127.0.1.1
B 127.0.2.1
C 127.0.3.1
```

and are conditional on unprivileged host bind preflight.

## Closure requirement

Closure must replace the registration token with one exact Plan-226 terminal,
record the baseline target-job skip reason, and if the topology correction is
authorized record the corrected A/B/C RouterInfo hosts and the resulting
lookup/reply/store/send stages.

Until then M6 Java interoperability remains unclaimed.
