# Plan 227 status — M6 Java explicit one-hop client-tunnel corrective

Status: **`registered-ready-m6-java-explicit-one-hop-client-tunnel-corrective`**.

Plan of record:
[`227-m6-java-explicit-one-hop-client-tunnel-corrective.md`](../../implementation/mixed-router-interop/227-m6-java-explicit-one-hop-client-tunnel-corrective.md).

## Registration basis

Plan 226 closed with the exact target-job terminal:

```text
P226-BASELINE-B-ZERO-HOP-UNKNOWN
```

The exact-pinned Java path shows this is caused by the current raw helper's
zero-hop outbound client tunnel combined with Java's intentional client-NetDB
RouterInfo separation.

Pinned/current Java I2P also provides `explicitPeers` as a public I2CP
debug/testing option. Through ordinary SessionConfig processing it can select a
known/selectable Router C for a genuine one-hop client tunnel without
manufacturing fast/high-capacity profile state.

Plan 227 therefore owns a bounded reference-harness corrective:

- prove Router C is valid/selectable in A's main NetDB;
- configure only the raw helper for one-hop inbound/outbound tunnels;
- scope `explicitPeers` to Router C through normal I2CP client options;
- prove live installed one-hop client tunnels read-only;
- rerun the frozen destination/reverse-send lane;
- stop at the first new exact boundary.

## Upstream testing alignment

The controlled lane retains Java I2P's local-router testing conventions:

- separate RouterContexts;
- loopback transport;
- `i2np.allowLocal=true`;
- real SSU2 rather than VMComm;
- public I2CP SessionConfig options;
- no Java source patching or profile/NetDB/tunnel state injection.

The Plan-226 distinct-loopback topology remains unadmitted because IP-close
filtering was disproved as the active boundary.

## Current authority

```text
plan_226 = passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary
plan_227 = registered-ready-m6-java-explicit-one-hop-client-tunnel-corrective

plan_201 = blocked-pending-plan227-explicit-one-hop-client-tunnel-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan227
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 227-m6-java-explicit-one-hop-client-tunnel-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

## Registration constraints

Plan 227 does not authorize:

- i2pr production protocol changes;
- Java patches/reflection/private-state mutation;
- profile tier/score manipulation;
- RouterInfo insertion into client NetDB;
- direct tunnel installation;
- VMComm;
- `netDb.alwaysQuery`;
- public I2P;
- Plan-226 distinct-loopback topology;
- Streaming-helper changes;
- reverse-delivery timeout changes.

Until closure, M6 Java interoperability remains unclaimed.
