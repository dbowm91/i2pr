# Plan 228 status — M6 Java client-tunnel build-path attribution

Status: **`registered-ready-m6-java-client-tunnel-build-path-attribution`**.

Plan of record:
[`228-m6-java-client-tunnel-build-path-attribution.md`](../../implementation/mixed-router-interop/228-m6-java-client-tunnel-build-path-attribution.md).

## Registration basis

Plan 227 closed with:

```text
P227-EXPLICIT-ONE-HOP-NOT-BUILT
```

on all three counted attempts at final implementation SHA
`b59bf5b77b33fab7784e4c8bfba24b7f5e46273b`.

Retained facts:

- Router C is present and valid in Router A's main NetDB;
- `ProfileOrganizer.isSelectable(C)=true`;
- Router C is not banlisted;
- the raw helper receives ordinary I2CP
  `inbound.explicitPeers` / `outbound.explicitPeers`;
- the helper requests one-hop inbound/outbound client tunnels with zero-hop
  disabled;
- stock Java reaches its approximately five-minute
  `I2PSession.connect()` ceiling without installing either client tunnel;
- Plan 227 did not proceed to the NetDB lookup or reverse-delivery stage.

Exact-pinned Java source shows several distinct pre-install stages that Plan
227 did not discriminate:

```text
client pool wants build
  -> selector/config creation
  -> BuildExecutor scheduling
  -> paired tunnel selection
  -> build-message creation
  -> A dispatch
  -> C receive/decrypt/respond
  -> A receive/decrypt statuses
  -> local join/install
```

For client tunnels, pinned `BuildRequestor` requires a paired tunnel. If no
opposite-direction client tunnel exists yet, Java falls back to router/
exploratory tunnel infrastructure. Absence of a suitable paired tunnel ends
the attempt as `OTHER_FAILURE` before Router C receives a build request.

Plan 228 therefore owns attribution only. It must locate the earliest missing
stage and stop without implementing a workaround.

## Current authority

```text
plan_227 = passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary
plan_228 = registered-ready-m6-java-client-tunnel-build-path-attribution

plan_201 = blocked-pending-plan228-client-tunnel-build-path-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan228
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 228-m6-java-client-tunnel-build-path-attribution
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

## Registration constraints

Plan 228 does not authorize:

- i2pr production changes;
- Java source patches;
- profile/tier manipulation;
- NetDB mutation;
- direct tunnel installation;
- exploratory/client tunnel setting changes;
- paired-tunnel policy overrides;
- VMComm;
- `netDb.alwaysQuery`;
- timeout increases;
- topology changes;
- Streaming-helper execution or changes.

Closure must replace this registration token with exactly one Plan-228
attribution terminal and update the dependency/unblock audit.
