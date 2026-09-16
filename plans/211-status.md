# Plan 211 status — M10 product-only remote HTTP/IRC acceptance harness

Status: **`retained-source-harness-blocked-by-plan212`**.

Plan of record: [`211-m10-product-only-remote-http-irc-final-acceptance-corrective.md`](211-m10-product-only-remote-http-irc-final-acceptance-corrective.md).

Plan 211's source-side harness improvements are retained, but the prior `passed-m10-product-only-remote-http-and-irc-application-closure-source-shipped` interpretation is superseded by Plan 212.

## Retained Plan 211 work

Plan 211 validly fixed the acceptance harness by:

- constructing real enabled `HttpClient` + `IrcClient` service specs instead of an empty `ServiceTunnelSet`;
- extracting only public i2pd service Destination material from generated server tunnel files;
- mapping the HTTP alias to the actual i2pd HTTP Destination;
- invoking system curl and exact-pinned jaraco/irc through the product listeners;
- recording command/fixture/per-operation subfacts;
- fixing the `record_guarded` boolean/exit-code bug;
- rejecting lower-stack construction/manual observation injection in the counted driver.

Those changes should not be rewritten unless Plan 212 API changes require a mechanical adaptation.

## Why the row remains blocked

A post-Plan-211 source audit found that the product underneath the harness still uses `SamLocalProductFabric` tunnel/LS2 material for service bridge construction, does not install the real exploratory tunnel roles as per-service network state, does not automatically bind real inbound receive ids to service runtimes, and does not complete `DestinationDispatcher::pop_payload` -> `StreamingDestinationAdapter::receive` into the canonical service `StreamingManager`.

Therefore the current full-lane failure cannot be classified as environment-only. The product seam must be corrected first.

## Corrected authority

```text
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-blocked-by-plan212
plan_212 = registered-executable

m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## Requalification rule

After Plan 212 proves generic router-backed Direction A + Direction B against exact-pinned unmodified i2pd, rerun the existing Plan 211 lane on the same implementation head (or a mechanical follow-up head) and require every mandatory HTTP/IRC subfact and aggregate row to pass.

Only then may this status become:

```text
plan_211 = passed-m10-product-only-remote-http-and-irc-application-closure
milestone10_final_acceptance = closed-via-plan211-after-plan212
```
