# Plan 192 status — M6 i2pd-compatible I2CP-style Data body wire-format corrective

Status: **`registered-m6-i2cp-wire-format-corrective`**. Plan 191
ran the corrected `run-destination.sh` against exact-pinned i2pd
2.61.0 and stopped at inbound-delivery boundary E: i2pd's
`ClientDestination::HandleDataMessage` parses an I2CP-style Data
header + gzip-wrapped datagram payload, but i2pr emits a raw
16-byte-standard I2NP Data body whose first four bytes are
misread by i2pd as the length field and overflow the available
buffer, so the i2pd SAM bridge never observes a
`DATAGRAM RECEIVED` line. Plan 192 owns the narrow protocol-layer
corrective for that boundary.

Plan of record:
[`plans/192-m6-i2cp-wire-format-corrective.md`](192-m6-i2cp-wire-format-corrective.md).

## Current authority

```text
plan_192 = registered-m6-i2cp-wire-format-corrective
plan_191 = stopped-by-inbound-delivery-boundary-E (4 inbound-delivery rows documented; 2 rows recorded blocked)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (3 destination rows flipped blocked -> passed)
plan_189 = registered-blocked-by-plan188-and-plan190-and-plan191-and-plan192 (cross-family scaffold only; Java qualification not started)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190)
plan_187 = blocked-by-m6-build-reply-interop-gap (5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows now blocked on plan191; full inbound-delivery layer blocked on plan192)

m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-blocked-plan191-then-plan192
m6_inbound_netdb_reply_path_correction = passed-via-plan190 (typed InboundGatewayRoute + daemon-owned adapter; remote lane flips 3 destination rows blocked -> passed)
m6_inbound_destination_delivery_boundary_E = stopped-pending-plan192 (i2pd-compatible I2CP-style Data body wire-format)
m6_inbound_destination_delivery = blocked-pending-plan192
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 192 (resolve inbound-delivery boundary E wire-format)
```

## What landed

Registration only: no `crates/` or `Cargo.lock` changes. The
Plan 190 implementation, harness boundary, and external evidence
lane are unchanged. Plan 192 inherits Plan 190's typed
`InboundGatewayRoute`, the daemon-owned
`reply_path_for_inbound_route` adapter, the Plan 187 destination
message-plane seams, and the
Plan 191-resilient `crates/i2pr-daemon/tests/destination_tunnel_external.rs`
that no longer panics on a silent SAM bridge — it now records
distinct evidence keys (`reference-received-timeout`,
`destination-inbound-send-failed`,
`inbound-delivery-boundary-E-stop`) and the
`tests/integration/m6-interop/run-destination.sh` records the
two inbound-delivery rows as `blocked` with the stop provenance
instead of failing the driver.

The Plan 191 `run-destination.sh` invocation lands the
following command-derived evidence:

```text
inbound-reply-path gateway_matches_reference=true gateway_tunnel=38401 local_receive=38402 ids_distinct=true
lease-lookup-completed leases=3 published=… expires=…
ls2-publication-tunnel cells=1
destination-outbound-delivered cells=1 payload_len=27
reference-received-timeout            timeout after 45s (Plan 191 inbound-delivery boundary E)
destination-inbound-send-failed       send_status=timeout (Plan 191 inbound-delivery boundary E)
inbound-delivery-boundary-E-stop      i2pd HandleDataMessage parses I2CP-style Data body, i2pr emits raw 16-byte standard I2NP Data body (m6 i2cp-wire-format-corrective)
direct-rejected                       true
liveness-first-test                   passed
shutdown-baseline                     true
external-lease-lookup-tunnel     = passed (retained from Plan 190)
external-ls2-publication-tunnel  = passed (retained from Plan 190)
external-destination-outbound    = passed (retained from Plan 190)
external-reference-received      = blocked (Plan 191 §6 stop; m6 i2cp-wire-format-corrective)
external-destination-inbound     = blocked (Plan 191 §6 stop; m6 i2cp-wire-format-corrective)
external-direct-rejected         = passed (no longer occluded)
external-liveness-first-test     = passed (no longer occluded)
```

## External-lane disposition

`external-reference-received` and `external-destination-inbound`
flip from `blocked` to `passed` only after Plan 192 closes the
I2CP-style Data body wire-format layer. Both rows must flip
together in a fresh `run-destination.sh` against the exact-pinned
i2pd 2.61.0; no already-passed Plan 187 / Plan 188 / Plan 190 /
Plan 191 row may regress. If Plan 192 reveals a previously-hidden
layer defect (e.g., a `STYLE=DATAGRAM` from-Identity requirement
that would force a follow-up plan to reconstruct an ElGamal/DSA
identity), the plan stops there and registers a narrower
follow-up without claiming closure; the corrected inner I2NP
short-transport format and the corrected I2CP-style Data body
are preserved.

## Stop conditions

Plan 192 stops without claiming closure if the corrected run
exhibits a fresh protocol-layer defect not addressed by the
9-byte short-transport inner envelope, the I2CP-style Data body,
or the gzip-no-compression wrapper. Per Plan 190 §10, no
`milestone6_interoperable = passed-via-plan192` claim is made
until every Plan 187 / Plan 188 / Plan 190 `passed` row plus
the four inbound-delivery rows pass in one corrected lane.

## Handoff

Plan 192 is the next executable plan. If it passes, the deferred
`plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming pass
becomes executable; Plan 189 (Java I2P second-family
qualification) depends on Plan 188 + Plan 192 + Streaming
closing first. M10 final acceptance stays open.

```text
plan_192 = registered-m6-i2cp-wire-format-corrective
plan_191 = stopped-by-inbound-delivery-boundary-E (retained)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (retained)
plan_188 = blocked-by-plan191-and-plan192
plan_189 = remains-blocked-until-plan188-and-plan191-and-plan192-and-streaming-close
next_executable_plan = 192
```
