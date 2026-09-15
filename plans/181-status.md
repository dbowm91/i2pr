# Plan 181 status — M10 independent application/service interoperability

Status: **`passed-local-matrix-remote-rows-blocked-pending-plan208-and-plan209`**.

Plan of record: [`181-m10-independent-application-and-service-interop-final-closure.md`](181-m10-independent-application-and-service-interop-final-closure.md).

The retained Plan 181 local/independent-client matrix remains valid: the 29 local rows continue to represent command-derived evidence for generic, HTTP, SOCKS5, IRC, restart, policy, and cleanup behavior on the local/co-owned M10 product path.

The remote rows remain non-authoritative after the post-Plan-207 call-graph audit.

Current row authority:

```text
m10_local_rows = passed (29/29 retained)
m10-remote-destination-streaming-composition = blocked/non-authoritative pending Plan 208
remote-independent-http-eepsite = blocked/non-authoritative pending Plan 209
remote-independent-irc-service = blocked/non-authoritative pending Plan 209
```

Why:

- Plan 206 added real remote backend seams, but the normal Plan 182 delivery driver still terminates a non-co-owned peer through its local `unknown_peer` branch instead of invoking them.
- Plan 207 added real curl/jaraco clients, but its counted driver still constructs and drives a parallel lower i2pr Streaming/router stack and manually advances legacy remote transport counters.

Current plan authority:

```text
plan_181 = passed-local-matrix-remote-rows-blocked-pending-plan208-and-plan209
plan_206 = retained-partial-executable-backend-seams-superseded-by-plan208
plan_207 = retained-partial-real-application-client-harness-superseded-by-plan209
plan_208 = registered-executable-m10-production-delivery-driver-remote-route-integration
plan_209 = registered-blocked-by-plan208
milestone10_independent_application_clients = local-only-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Do not delete or weaken the retained 29 local rows.

Execution graph:

```text
plan205 || plan208
plan208 -> plan209
closed Java branch + plan209 -> plan204 convergence
```
