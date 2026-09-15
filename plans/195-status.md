# Plan 195 status — M10 remote independent service interoperability and final closure

Status: **`blocked-m10-remote-independent-service-pending-plan208-and-plan209`**.

Plan of record: [`195-m10-remote-independent-service-final-closure.md`](195-m10-remote-independent-service-final-closure.md).

Plan 195's positive remote-service criterion is still open after the post-Plan-207 audit.

Retained facts:

- Plan 181's 29 local rows remain passed.
- Plan 193 proves the lower exact-pinned i2pd mixed-router Streaming stack.
- Plan 206 landed useful executable remote-backend primitives.
- Plan 207 landed useful real curl/jaraco application-client and fixture evidence scaffolding.

Remaining defects:

- the normal Plan 182 service delivery driver does not yet invoke the Plan 206 remote route for a non-local queued request;
- the Plan 207 counted application driver still constructs/drives a parallel lower i2pr Streaming/router stack and manually advances legacy transport counters.

Current authority:

```text
plan_195 = blocked-m10-remote-independent-service-pending-plan208-and-plan209
plan_206 = retained-partial-executable-backend-seams-superseded-by-plan208
plan_207 = retained-partial-real-application-client-harness-superseded-by-plan209
plan_208 = registered-executable-m10-production-delivery-driver-remote-route-integration
plan_209 = registered-blocked-by-plan208
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Plan 195 requires no separate implementation pass while Plans 208/209 execute. They are the narrow corrective realization of its remaining remote-service requirement.

On Plan 209 success, Plan 195 may transition to:

```text
plan_195 = evidence-passed-m10-remote-independent-service-via-plan208-and-plan209-pending-plan204
```

Only Plan 204 may perform final cross-milestone authority closure, and only after the independent Java branch has also closed.
