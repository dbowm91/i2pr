# Plan 204 status — final M6/M10 evidence and authority convergence

Status: **`blocked-on-independent-java-m6-branch-and-m10-plan212-plus-plan211-requalification`**.

Plan of record: [`204-m10-final-closure-evidence-authority-and-documentation-normalization.md`](204-m10-final-closure-evidence-authority-and-documentation-normalization.md).

Plan 204 remains a later cross-milestone convergence/normalization pass. It must not hide product/evidence defects, and it must not force the independently testable M10 product to remain open solely because the Java M6 second-family branch is unresolved.

## Corrected execution graph

```text
Java / M6 branch:
  independent Plan 205 / evidence-driven successor -> M6 Java second-family closure

M10 branch:
  Plan 212
    -> real router-backed per-service Destination network material
    -> production inbound receive-id ownership
    -> per-service actual target LeaseSet2 lookup
    -> real server LS2 publication where required
    -> Garlic -> pop_payload -> StreamingDestinationAdapter::receive
    -> generic Direction A + Direction B external proof

  then retained Plan 211
    -> product-only curl HTTP remote row
    -> exact-pinned jaraco IRC remote row
    -> exact-head repeatability
    -> M10 closure

parallel outcomes:
  Plan 211 may close Milestone 10 independently after Plan 212
  Java branch may close Milestone 6 independently

later convergence:
  closed Java M6 branch + already-closed M10 -> Plan 204 authority/docs convergence
```

## Why Plan 212 supersedes the prior Plan 210/211 closure interpretation

The post-Plan-211 audit found:

- `ServiceTunnelManager::create_bridge_for_spec` still constructs every service bridge through `SamLocalProductFabric`, which explicitly generates synthetic localhost-only tunnel material;
- Plan 210 removed an explicit placeholder swap but did not replace the underlying synthetic bridge outbound role/local LS2 with router-backed service-Destination material;
- the real exploratory pair created by `ServiceProduct` is not installed into service runtimes;
- real inbound receive tunnel ids are not production-bound to those service runtimes;
- `dispatch_inbound_garlic_owned` stops after `DestinationDispatcher::dispatch_garlic_envelope` and does not drain destination payloads into the canonical service Streaming manager;
- router bootstrap still carries one application Destination hash while Plan 211 may configure multiple independent remote targets.

Therefore the Plan 211 full-lane failure cannot be classified as environment-only yet.

## Current authority

```text
plan_200 = retained diagnostic/evidence pass
plan_201 = retained/in-progress Java corrective history
plan_204 = blocked-on-independent-java-m6-branch-and-m10-plan212-plus-plan211-requalification
plan_205 = registered/in-progress Java second-family branch

plan_202 = retained-partial-routing-capability-surface
plan_203 = retained-partial-application-observation-scaffolding
plan_206 = retained-partial-executable-backend-seams
plan_207 = retained-partial-real-application-client-harness
plan_208 = retained-partial-production-call-graph-corrective
plan_209 = retained-partial-black-box-composition-harness
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-blocked-by-plan212
plan_212 = registered-executable

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
m10_remote_transport_core = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

The retained M10 local matrix stays passed. The remote rows remain non-authoritative until Plan 212 product proof and subsequent Plan 211 application proof are green.

## M10 closure authority

After Plan 212 generic Direction A + Direction B pass, authority may advance only to:

```text
plan_212 = passed-m10-router-backed-service-destination-and-canonical-inbound-streaming
m10_remote_transport_core = passed-via-plan212
plan_211 = retained-source-harness-ready-for-requalification
```

After the subsequent exact-head Plan 211 HTTP + IRC lane passes, Plan 211 may independently record:

```text
plan_211 = passed-m10-product-only-remote-http-and-irc-application-closure
m10_remote_application_interop = passed-via-plan211-after-plan212
milestone10_remote_service_interop = passed-via-plan212-and-plan211
milestone10_final_acceptance = closed-via-plan211-after-plan212
next_product_layer = milestone11-planning
```

That transition does **not** require Java M6 second-family closure.

Plan 204 later consumes the already-closed M10 authority plus independently closed Java M6 authority and normalizes cross-milestone documentation. It must not downgrade or rerun a valid M10 closure merely because Java closure lands later.
