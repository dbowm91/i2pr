# Plan 207 status — genuine remote HTTP/IRC application interoperability corrective

Status: **`passed-m10-genuine-remote-http-and-irc-application-interop-superseded-by-plan209`**.

Plan of record: [`207-m10-genuine-remote-http-and-irc-application-interop-corrective.md`](207-m10-genuine-remote-http-and-irc-application-interop-corrective.md).

## Closed authority

Plan 207 fixed an important part of the earlier Plan 203 evidence defect and is now closed as a historical scaffold for the application layer:

- real system `curl` is invoked as a subprocess;
- exact-pinned clean jaraco/irc is invoked through its public Python API;
- application subfacts are derived from client exit/results and loopback fixture facts;
- the counted driver no longer calls `record_remote_application_observation`;
- aggregate HTTP/IRC rows are fail-closed when mandatory application subfacts are absent;
- the full lane provisions exact-pinned i2pd server tunnels to harness-owned fixtures.

The Plan 207 driver is retained on disk as a historical scaffold and is no longer
authoritative for the counted positive rows: Plan 209 replaced the shadow
Streaming/router stack with the production composition helper, drove the same
application clients, and now owns the counted aggregate rows. The static
checker (`scripts/check-service-tunnel-acceptance-evidence.sh`) still requires
the Plan 207 driver file to exist for backwards compatibility but the runner
script invokes the Plan 209 driver instead.

## Why Plan 207 became superseded

The counted Plan 207 driver still imports and directly drives a second i2pr
lower stack beside the `ServiceTunnelManager`, including
destination/Streaming/router components such as `StreamingManager`,
`StreamingDestinationAdapter`, `DestinationTunnelCoordinator`, exploratory
build state, and direct router-delivery requests.

Plan 209 closed this defect by exposing the single production composition
function (`ServiceProduct::start` / `poll_inbound` / `remote_counters`) that
both Plan 208 and Plan 209 drivers consume; the counted application driver
becomes a thin harness that never constructs any of the lower-stack types
listed in Plan 209 §5.

## Authority transitions

```text
plan_203 = retained-partial-application-observation-scaffolding
plan_207 = passed-m10-genuine-remote-http-and-irc-application-interop-superseded-by-plan209
plan_208 = passed-m10-production-delivery-driver-remote-route-integration
plan_209 = passed-m10-product-only-remote-http-and-irc-application-interop
plan_181 = passed-m10-independent-application-and-service-interop-via-plan209
plan_195 = evidence-passed-m10-remote-independent-service-via-plan208-and-plan209-pending-planplan204-m10-final-closure-evidence-authority-and-documentation-normalization
m10_remote_application_interop = passed-via-plan209
milestone10_remote_service_interop = passed-via-plan208-and-plan209-pending-planplan204-m10-final-closure-evidence-authority-and-documentation-normalization
```

Final milestone closure remains owned by Plan 204 after the Java branch also closes.

Historical implementation head for Plan 207 was `f9d8e886fda9910bd2afbc86f35ec4c80dd852c5`; any earlier `c67dc5b... current head` wording is stale and non-authoritative.
