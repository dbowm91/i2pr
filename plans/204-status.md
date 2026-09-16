# Plan 204 status — final M6/M10 evidence and authority convergence

Status: **`blocked-on-independent-java-m6-branch-and-m10-plan210-211-correctives`**.

Plan of record: [`204-m10-final-closure-evidence-authority-and-documentation-normalization.md`](204-m10-final-closure-evidence-authority-and-documentation-normalization.md).

Plan 204 remains a later cross-milestone convergence/normalization pass. It must not hide product/evidence defects, and it must not force the independently testable M10 product to remain open solely because the Java M6 second-family branch is still unresolved.

## Corrected execution graph

```text
Java / M6 branch:
  plan205 -> evidence-driven successor only if required -> M6 Java second-family closure

M10 branch:
  plan210 -> real service-Destination tunnel material, destination-aware lookup,
             complete inbound Garlic -> canonical service Streaming,
             bidirectional generic external product proof
  plan211 -> product-only curl/jaraco remote HTTP/IRC evidence -> M10 closure

parallel outcomes:
  plan211 may close Milestone 10 independently
  Java branch may close Milestone 6 independently

later convergence:
  closed Java M6 branch + already-closed M10 -> plan204 authority/docs convergence
```

## Why Plans 210/211 replace the prior 208/209 closure interpretation

The post-Plan-209 audit found:

- Plan 208 correctly routed the normal service delivery local-miss branch into the remote backend, but counted remote composition can still depend on service bridge tunnel/LeaseSet material created by `SamLocalProductFabric` and the `dummy_outbound_tunnel()` placeholder;
- Plan 209 correctly removed lower-stack construction from the counted application driver, but `ServiceProduct` still does not provision real destination tunnel material for each service Destination;
- `ServiceProduct` performs a LeaseSet lookup using `SHA256(reference.router_info_bytes)` rather than the actual remote service Destination hash;
- recovered remote Garlic is not yet actually delivered through the owning service DestinationDispatcher/ECIES state into the same canonical service `StreamingManager` used by the application profile;
- Plan 209 HTTP/IRC acceptance still requires real enabled M10 service specs, actual i2pd public Destinations, and causal command/fixture/per-application counter-delta evidence.

## Current authority

```text
plan_200 = retained diagnostic/evidence pass
plan_201 = retained/in-progress Java corrective history
plan_204 = blocked-on-independent-java-m6-branch-and-m10-plan210-211-correctives
plan_205 = registered/in-progress Java second-family branch

plan_202 = retained-partial-routing-capability-surface
plan_203 = retained-partial-application-observation-scaffolding
plan_206 = retained-partial-executable-backend-seams
plan_207 = retained-partial-real-application-client-harness
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = registered-executable-m10-real-service-destination-network-material-and-inbound-streaming
plan_211 = registered-blocked-by-plan210

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
m10_remote_transport_core = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

The three M10 remote rows remain non-authoritative until re-proven through Plans 210/211:

```text
m10-remote-destination-streaming-composition
remote-independent-http-eepsite
remote-independent-irc-service
```

## M10 closure authority

Plan 211 may independently record:

```text
m10_remote_transport_core = passed-via-plan210
m10_remote_application_interop = passed-via-plan211
milestone10_remote_service_interop = passed-via-plan210-and-plan211
milestone10_final_acceptance = closed-via-plan211
next_product_layer = milestone11-planning
```

That transition requires command-derived exact-head evidence defined in Plans 210/211 and does **not** require the Java M6 second-family row to be green.

Plan 204 later consumes that already-closed M10 authority together with the independently closed Java M6 branch and normalizes cross-milestone documentation. It must not downgrade M10 or rerun M10 merely because Java closure landed later.

## Plan 204 convergence requirements after this correction

Plan 204 itself may complete only after:

1. Java M6 second-family closure is honest and command-derived.
2. Plan 210 status is passed with bidirectional generic remote product evidence.
3. Plan 211 status is passed with remote HTTP and IRC product evidence.
4. M10 is already closed via Plan 211.
5. Retained Plan 181 local 29-row matrix remains green.
6. M6 cross-family evidence gate is green.
7. Routine CI/static/dependency/docs floors are green.
8. No authority file claims stronger evidence than exists on the relevant closure heads.

Only then may Plan 204 record cross-milestone convergence such as:

```text
milestone6_interoperable = passed-via-retained-i2pd-and-closed-java-second-family
milestone10_final_acceptance = closed-via-plan211-retained
m6_m10_authority_convergence = closed-via-plan204
next_product_layer = milestone11-planning
```

Until each branch is independently proven, keep its status open without weakening the other branch's criteria.
