# Plan 181 status — M10 independent application/service interoperability

Status: **`passed-local-matrix-remote-rows-blocked-pending-plan210-and-plan211`**.

Plan of record: [`181-m10-independent-application-and-service-interop-final-closure.md`](181-m10-independent-application-and-service-interop-final-closure.md).

The retained Plan 181 local/independent-client matrix remains valid: the 29 local rows continue to represent command-derived evidence for generic, HTTP, SOCKS5, IRC, restart, policy, and cleanup behavior on the local/co-owned M10 product path.

The remote rows are non-authoritative pending Plans 210/211.

Current row authority:

```text
m10_local_rows = passed (29/29 retained)
m10-remote-destination-streaming-composition = blocked pending plan210
remote-independent-http-eepsite = blocked pending plan211
remote-independent-irc-service = blocked pending plan211
```

Reason:

- Plan 208 fixed the production delivery call graph but did not prove real service-Destination tunnel material or complete inbound Garlic -> canonical service Streaming dispatch.
- Plan 209 removed the counted driver's shadow stack but did not fix those product defects, and its HTTP/IRC evidence still needs real enabled service specs, real reference public Destinations, and command/fixture/per-application counter-delta gating.

Current authority:

```text
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = registered-executable
plan_211 = registered-blocked-by-plan210
milestone10_independent_application_clients = local-only-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Do not delete, weaken, or rerun the retained 29 local rows merely to execute the remote corrective path.

Execution graph:

```text
plan210 -> plan211 -> M10 closure

Java/M6 proceeds independently in parallel;
closed Java branch + already-closed M10 later converge through plan204.
```
