# Plan 242 status — M6 Java Streaming stock one-hop selector semantics corrective

Status: **registered-ready-m6-java-streaming-stock-one-hop-selector-semantics-corrective**.

Plan of record:
plans/implementation/mixed-router-interop/242-m6-java-streaming-stock-one-hop-selector-semantics-corrective.md

## Registration basis

Plan 241 closed at:
passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary

Its decisive attempt proved:
- Plan-241 one-hop SessionConfig active;
- one inbound and one outbound client tunnel installed;
- zero-hop absent in both directions;
- neither installed tunnel exact-via-C;
- the lookup driver was blocked by the exact-via-C gate.

Exact-pinned Java source review now proves that explicitPeers is intentionally probabilistic. TunnelPeerSelector.shouldSelectExplicit() reads explicitPeers but returns true only when random.nextInt(4) == 0. Only that branch calls selectExplicit(). Otherwise ClientPeerSelector.selectPeers() uses the ordinary stock fast-peer path.

Therefore exact-via-C was an over-constraint in the Plan-241 harness. It is not required to bypass the Plan-240 zero-hop/unknown-RI guard. A genuine non-zero-hop outbound client tunnel is sufficient for that specific guard.

Plan 242 corrects the harness interpretation and immediately resumes the already-instrumented Plan-240/241 lookup path. It does not patch Java selection, manipulate RNG, force Router C, change topology, change publication, or authorize production i2pr work.

## Current authority

plan_241 = passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary
plan_242 = registered-ready-m6-java-streaming-stock-one-hop-selector-semantics-corrective

plan_201 = blocked-after-plan241-a-b-build-stage-pending-plan242-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan242
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 242-m6-java-streaming-stock-one-hop-selector-semantics-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
