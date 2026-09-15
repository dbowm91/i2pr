# Plan 204 status — final M6/M10 evidence and authority convergence

Status: **`blocked-on-java-branch-and-plan208-209-m10-correctives`**.

Plan of record: [`204-m10-final-closure-evidence-authority-and-documentation-normalization.md`](204-m10-final-closure-evidence-authority-and-documentation-normalization.md).

Plan 204 remains a convergence/authority pass only. It must not hide product or evidence defects.

A post-Plan-207 source-level audit found that the M10 remote branch is still not closed:

- Plan 206 added executable backend seams, but the normal Plan 182 service delivery driver still terminates a non-co-owned queued request through the local `unknown_peer` branch instead of invoking the remote route;
- Plan 207 added real curl/jaraco application clients, but its counted driver still builds and drives a parallel lower i2pr Streaming/router stack and manually advances legacy remote transport counters.

The next M10 corrections are therefore Plans 208 and 209.

Execution graph:

```text
Java branch:
  plan205 -> evidence-driven successor only if required

M10 branch:
  plan208 -> production delivery driver actually routes remote queued requests
  plan209 -> product-only curl/jaraco acceptance with no shadow i2pr stack

convergence:
  closed Java branch + plan209 -> rerun plan204 exact-head closure
```

## Corrected authority

```text
plan_200 = retained diagnostic/evidence pass
plan_201 = in-progress Java corrective history
plan_202 = retained-partial-routing-capability-surface
plan_203 = retained-partial-application-observation-scaffolding
plan_204 = blocked-on-java-branch-and-plan208-209-m10-correctives
plan_205 = registered/in-progress Java second-family corrective branch
plan_206 = retained-partial-executable-backend-seams-superseded-by-plan208
plan_207 = retained-partial-real-application-client-harness-superseded-by-plan209
plan_208 = registered-executable-m10-production-delivery-driver-remote-route-integration
plan_209 = registered-blocked-by-plan208

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
m10_remote_transport_core = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

The three previously promoted M10 remote rows remain non-authoritative until re-proven through Plans 208/209:

```text
m10-remote-destination-streaming-composition
remote-independent-http-eepsite
remote-independent-irc-service
```

## Final convergence requirements

Plan 204 may close only after all of the following are true on one exact head:

1. The mandatory Java M6 second-family branch is closed with honest evidence.
2. Plan 208 proves that the existing manager-owned service delivery driver routes a real non-local queued Streaming request through the shared remote backend.
3. Plan 208 proves real inbound recovered remote traffic reaches the actual owning service runtime.
4. Plan 208 generic-client and generic-server product proofs pass without a parallel i2pr Streaming/router stack.
5. Plan 209 proves actual system curl HTTP traffic through the production remote product path.
6. Plan 209 proves exact-pinned jaraco/irc traffic through the production remote product path.
7. Plan 209's counted driver contains no direct lower-stack construction or manual transport-success counter increments.
8. The retained 29 local M10 rows remain passed.
9. The full remote lane has zero mandatory blocked/failed/missing rows.
10. The M6 cross-family final evidence gate is green.
11. Routine CI/static/dependency/docs floors are green.
12. No authority file claims a stronger status than the same-head evidence.

Only then may Plan 204 record:

```text
milestone6_interoperable = passed-via-retained-i2pd-and-closed-java-second-family
m10_remote_transport_core = passed-via-plan208
milestone10_remote_service_interop = passed-via-plan208-and-plan209
milestone10_final_acceptance = closed-via-plan204
next_product_layer = milestone11-planning
```

Until then, final closure is prohibited.
