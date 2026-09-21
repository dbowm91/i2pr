# Plan 230 status — M6 Java profile-population-path attribution

Status: **`registered-ready-m6-java-profile-population-path-attribution`**.

Plan of record:
[`230-m6-java-profile-population-path-attribution.md`](../../implementation/mixed-router-interop/230-m6-java-profile-population-path-attribution.md).

## Registration basis

Plan 229 closed with:

```text
P229-C-NOT-EXPLORATORY-ELIGIBLE
```

on all four counted attempts (SHAs `00dc368` / `10d1015`).

Retained facts:

- A=service, B=publication, C=transit roles proven live; C proven
  non-floodfill; Router A effective exploratory settings match the
  stock small-router profile (`1/1/1/1`);
- `P200-H-publication-path-passed` on every attempt (ordinary
  authenticated DatabaseStore bootstrap works);
- Router C present and valid in A's main NetDB;
  `ProfileOrganizer.isSelectable(C)=true`;
- `profile_present=false`, `profile_count=0`, `not_failing_count=0`
  after a bounded 60 s in-flight wait;
- pinned bytecode shows `isSelectable` is RouterInfo-fact based while
  `selectAllPeers` reflects the tier maps, so selectability never
  implied tier membership;
- helper never started; no exploratory snapshot; no P228 trace; lookup
  lane never entered.

The tier population that pinned `ExploratoryPeerSelector` selects from
is empty. Plan 230 attributes, through ordinary stock-Java activity
only, how (and whether) the controlled topology populates it.

## Current authority

```text
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = registered-ready-m6-java-profile-population-path-attribution

plan_201 = blocked-pending-plan230-profile-population-path-attribution-after-plan229-not-eligible
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan230
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 230-m6-java-profile-population-path-attribution
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

## Authorized attribution

Plan 230 may add only read-only harness diagnostics observing the
organizer tier population over time and its correlation with ordinary
stock activity (comm establishment, DatabaseStore handling,
exploratory scheduling, reorganization).

It then records exactly one terminal (`P230-PROFILE-POPULATED`,
`P230-PROFILE-ABSENT`, or `P230-OBSERVABILITY-GAP`) and stops before
exploratory polling and helper execution.

## Prohibited shortcuts

Plan 230 does not authorize:

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
- Plan-229 role/settings changes;
- Streaming-helper execution;
- target lookup/reverse-delivery qualification.

Closure must replace this registration token with exactly one Plan-230
terminal and update the unblock audit.
