# Plan 207 status — genuine remote HTTP/IRC application interoperability corrective

Status: **`retained-partial-real-application-client-harness-superseded-by-plan209`**.

Plan of record: [`207-m10-genuine-remote-http-and-irc-application-interop-corrective.md`](207-m10-genuine-remote-http-and-irc-application-interop-corrective.md).

## Retained work

Plan 207 fixed an important part of the earlier Plan 203 evidence defect:

- real system `curl` is invoked as a subprocess;
- exact-pinned clean jaraco/irc is invoked through its public Python API;
- application subfacts are derived from client exit/results and loopback fixture facts;
- the counted driver no longer calls `record_remote_application_observation`;
- aggregate HTTP/IRC rows are fail-closed when mandatory application subfacts are absent;
- the full lane provisions exact-pinned i2pd server tunnels to harness-owned fixtures.

These pieces should be retained.

## Why Plan 207 is not a pass

The counted Plan 207 driver still imports and directly drives a second i2pr lower stack beside the `ServiceTunnelManager`, including destination/Streaming/router components such as `StreamingManager`, `StreamingDestinationAdapter`, `DestinationTunnelCoordinator`, exploratory build state, and direct router-delivery requests.

The driver defines and uses its own transport-send helper to compose and submit Streaming cells. It also manually advances legacy remote transport counters such as `remote_stream_connect_started`, `remote_stream_established`, and `remote_outbound_requests`. The resulting `plan206_ok` proof can therefore be satisfied by the test-owned stack rather than by the actual M10 service listener's production delivery loop.

Real curl/jaraco clients are necessary but not sufficient when the network half of the same driver bypasses the product call graph.

## Corrected authority

```text
plan_203 = retained-partial-application-observation-scaffolding
plan_207 = retained-partial-real-application-client-harness-superseded-by-plan209
plan_208 = registered-executable-m10-production-delivery-driver-remote-route-integration
plan_209 = registered-blocked-by-plan208
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
```

Plan 209 owns the cleanup after Plan 208 lands. It must remove the shadow i2pr Streaming/router stack from the counted application driver and prove HTTP/IRC strictly through actual product listeners, fixture facts, and operation-derived Plan 208 counters.

Historical implementation head for Plan 207 was `f9d8e886fda9910bd2afbc86f35ec4c80dd852c5`; any earlier `c67dc5b... current head` wording is stale and non-authoritative.
