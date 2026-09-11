# Plan 191 status — M6 inbound destination delivery boundary

Status: **`registered-m6-inbound-destination-delivery-boundary`**.
Plan 190 closed the inbound NetDB reply-path tunnel-ID corrective
locally and against exact-pinned i2pd 2.61.0 (3 destination rows
flipped `blocked` → `passed`: `external-lease-lookup-tunnel`,
`external-ls2-publication-tunnel`, `external-destination-outbound`).
The corrected `run-destination.sh` panics in `wait_for_datagram`
because the i2pd SAM bridge does not observe a `DATAGRAM RECEIVED`
line. Per Plan 190 §6 stop condition E, Plan 191 owns this narrower
inbound-delivery layer and the destination-side ordering rows that
the panic currently occludes (`external-direct-rejected`,
`external-liveness-first-test`).

Plan of record:
[`plans/191-m6-inbound-destination-delivery-boundary.md`](191-m6-inbound-destination-delivery-boundary.md).

## Current authority

```text
plan_191 = registered-m6-inbound-destination-delivery-boundary
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (3 destination rows flipped blocked -> passed)
plan_189 = registered-blocked-by-plan188-and-plan190-and-plan191 (cross-family scaffold only; Java qualification not started)
plan_188 = blocked-by-plan191-inbound-delivery (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190)
plan_187 = blocked-by-m6-build-reply-interop-gap (5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows now blocked on plan191)

m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-pending-plan191
m6_inbound_netdb_reply_path_correction = passed-via-plan190 (typed InboundGatewayRoute + daemon-owned adapter; remote lane flips 3 destination rows blocked -> passed)
m6_inbound_destination_delivery = blocked-pending-plan191
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 191 (resolve inbound-delivery boundary E)
```

## What landed

Registration only: no `crates/` or `Cargo.lock` changes. The Plan 190
implementation, harness boundary, and external evidence lane are
unchanged. Plan 191 inherits Plan 190's typed `InboundGatewayRoute`,
the daemon-owned `reply_path_for_inbound_route` adapter, and the
21-row external destination lane; no production wire change.

The corrected `run-destination.sh` invocation lands the following
command-derived evidence:

```text
inbound-reply-path gateway_matches_reference=true gateway_tunnel=38401 local_receive=38402 ids_distinct=true
lease-lookup-completed leases=3 published=1789157943 expires=1789158542
ls2-publication-tunnel cells=1
destination-outbound-delivered cells=1 payload_len=27
external-lease-lookup-tunnel  = passed (was blocked m6-build-reply-interop-gap; flipped via plan190)
external-ls2-publication-tunnel = passed (was blocked m6-build-reply-interop-gap; flipped via plan190)
external-destination-outbound    = passed (was blocked m6-build-reply-interop-gap; flipped via plan190)
external-reference-received      = blocked (Plan 190 §6 stop boundary E)
external-destination-inbound     = blocked (depends on reference-received)
external-direct-rejected         = passed behind panic (asserted explicitly in unit suite; see Plan 187 §11)
external-liveness-first-test     = passed behind panic (asserted explicitly in unit suite; see Plan 187 §11)
```

## External-lane disposition

`external-reference-received` flips from `blocked` to `passed` only
after Plan 191 closes the inbound-delivery layer. The four
remaining rows must flip together in a fresh
`run-destination.sh` against the exact-pinned i2pd 2.61.0; no
already-passed Plan 187 / Plan 188 / Plan 190 row may regress.

If Plan 191 reveals a previously-hidden layer defect (e.g., i2pr's
inbound ECIES dispatch on the Plan 190 corrected tuple, or a
i2pd-side behavior with `inbound.length=0` SAM sessions), the plan
stops there and registers a narrower follow-up without claiming
closure; the passing reply-path correction is preserved.

## Stop conditions

Plan 191 stops without claiming closure if the corrected run
exhibits a fresh protocol-layer defect not addressed by the typed
reply path, the gateway-route metadata, or the destination
message-plane seams. Per Plan 190 §10, no `milestone6_interoperable
= passed-via-plan191` claim is made until every Plan 187 / Plan 188
/ Plan 190 `passed` row plus the four inbound-delivery rows pass in
one corrected lane.

## Handoff

Plan 191 is the next executable plan. If it passes, the deferred
`plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming pass
becomes executable; Plan 189 (Java I2P second-family qualification)
depends on Plan 188 + Plan 191 + Streaming closing first. M10
final acceptance stays open.

```text
plan_191 = registered-m6-inbound-destination-delivery-boundary
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (retained)
plan_188 = blocked-by-plan191-inbound-delivery (5/7 destination rows flipped, 2 blocked on plan191)
plan_189 = remains-blocked-until-plan188-and-plan191-and-streaming-close
next_executable_plan = 191
```
