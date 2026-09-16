# Plan 211 status — retained M10 product-only remote HTTP/IRC harness

Status: **`retained-source-harness-superseded-for-final-evidence-by-plan214`**.

Plan of record: [`211-m10-product-only-remote-http-irc-final-acceptance-corrective.md`](211-m10-product-only-remote-http-irc-final-acceptance-corrective.md).

Plan 211 remains useful source/history, but it is no longer the final acceptance authority by itself.

## Retained valid work

Plan 211 validly introduced or retained:

- real enabled `HttpClient` + `IrcClient` service specs rather than an empty `ServiceTunnelSet`;
- actual i2pd-owned public HTTP and IRC Destinations;
- HTTP alias mapping to the actual remote Destination;
- system curl for HTTP;
- exact-pinned jaraco/irc for IRC;
- product startup through `ServiceProduct` rather than the old lower-stack shadow router;
- operation-derived remote counter surfaces;
- fail-closed aggregate row handling in the shell runner;
- public/private boundary parsing for i2pd destination files.

Do not throw this harness away. Plan 214 is a narrow evidence/execution hardening pass over it.

## Qualification-integrity defects found after Plan 212

The current counted driver is not yet final authority because:

- it directly constructs a `_manager_placeholder = ServiceTunnelManager::new(...)`, which violates the intended black-box counted-driver boundary;
- i2pd/jaraco pin facts are stamped as literal success values in the Rust driver rather than bound to runner command verification;
- `irc-privacy-rewrite-derived` is a literal success value rather than fixture-derived;
- DCC blocking is inferred from absence of `DCC_SENT`, not from an explicit policy attempt plus target non-observation;
- HTTP target observation is inferred partly from client-side response facts rather than fresh target fixture records;
- HTTP POST/large-response rows do not yet have sufficiently strict target/expected-digest equality gates;
- HTTP status is approximated rather than captured as the actual curl HTTP status;
- target-port inputs are read but not causally tied to final evidence;
- blocking external subprocess calls are not guaranteed to keep `ServiceProduct::poll_inbound()` active while the clients are waiting;
- hosted exact-head repeatability has not passed.

These are qualification defects, not grounds to reopen the Plan 212 router architecture before Plan 213 executes.

## Current authority

```text
plan_211 = retained-source-harness-superseded-for-final-evidence-by-plan214
plan_212 = source-closure-landed-qualification-owned-by-plan213
plan_213 = registered-executable
plan_214 = registered-blocked-by-plan213

m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## Execution rule

1. Execute Plan 213 first and require generic router-backed Direction A + Direction B to pass twice on one exact hosted SHA.
2. Then execute Plan 214 to harden this retained harness and run HTTP/IRC on the same product architecture.
3. Only command + target-fixture + product-counter evidence may pass the two remote application aggregate rows.
4. Only two consecutive hosted full-lane passes on one exact SHA may close M10.

On terminal success, Plan 211 becomes retained evidence underneath Plan 214 rather than the final closure owner.
