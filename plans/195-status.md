# Plan 195 status — M10 remote independent service interoperability and final closure

Status: **`registered-blocked-by-plan199`**.

Plan of record:
[`plans/195-m10-remote-independent-service-final-closure.md`](195-m10-remote-independent-service-final-closure.md).

Plan 195 resumes the two remote service rows left blocked by Plan 181 after the local M10 product and independent local client matrix passed. It is incorporated into Plan 199 and must not execute until the unified M6 and M10 remote-transport criteria pass.

The earlier `unblocked-m10-remote-service-interop` interpretation is superseded. Plan 194's exact-pinned Java lane retained useful passing evidence, but its final run contained 22 mandatory LS2/destination/Streaming rows recorded `blocked`; Plan 194's own final acceptance criteria did not permit those rows to be converted into a bounded final pass. Plan 198 restores the fail-closed criterion and owns the public-Java-client completion path.

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_198 = blocked-public-java-client-leaseset2-publication
plan_199 = blocked-java-public-client-leaseset2-and-m10-remote-transport
plan_195 = registered-blocked-by-plan199

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 199
remaining_sequence = 199 (unified 198 -> 195)
```

Plan 195 becomes executable only after Plan 198 records:

```text
mandatory i2pd M6 rows = all passed
mandatory Java M6 rows = all passed
mandatory blocked/failed/missing rows = 0
exact-head routine CI = success
exact-head full two-family M6 external workflow = success
milestone6_interoperable = passed-via-plan193-and-plan198
```

On Plan 198 pass, Plan 195 may return to:

```text
plan_195 = unblocked-m10-remote-service-interop
next_executable_plan = 195
```

On subsequent Plan 195 pass:

```text
milestone10_remote_service_interop = passed-via-plan195
milestone10_final_acceptance = closed-via-plan195
next_product_layer = milestone11-planning
```
