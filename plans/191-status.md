# Plan 191 status — M6 inbound destination delivery boundary

Status: **`stopped-by-inbound-delivery-boundary-E`**. Plan 190
closed the inbound NetDB reply-path tunnel-ID corrective locally
and against exact-pinned i2pd 2.61.0 (3 destination rows flipped
`blocked` → `passed`: `external-lease-lookup-tunnel`,
`external-ls2-publication-tunnel`,
`external-destination-outbound`). The corrected
`run-destination.sh` reached the destination outbound delivery
(`destination-outbound-delivered cells=1 payload_len=27`) but the
i2pd SAM bridge never observed a `DATAGRAM RECEIVED` line. Plan
191 ran the inbound-delivery layer, isolated the wire-format
defect, recorded the four inbound-delivery rows as `blocked`
with stop provenance, and registered a narrower follow-up plan.

Plan of record:
[`plans/191-m6-inbound-destination-delivery-boundary.md`](191-m6-inbound-destination-delivery-boundary.md).
Narrower follow-up:
[`plans/192-m6-i2cp-wire-format-corrective.md`](192-m6-i2cp-wire-format-corrective.md).

## Current authority

```text
plan_191 = stopped-by-inbound-delivery-boundary-E
plan_192 = registered-m6-i2cp-wire-format-corrective (next executable)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (3 destination rows flipped blocked -> passed)
plan_189 = registered-blocked-by-plan188-and-plan190-and-plan191-and-plan192 (cross-family scaffold only; Java qualification not started)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190)
plan_187 = blocked-by-m6-build-reply-interop-gap (5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191; full inbound-delivery layer blocked on plan192)

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

Plan 191 ran the corrected `run-destination.sh` against
exact-pinned i2pd 2.61.0 (commit `635b013a612ff47278ef02acf8580a28e10e26c5`,
loopback + unpublished, i2pd configured with
`notransit=false,floodfill=true` plus loopback SAM). The
`destination_message_plane_against_i2pd` driver no longer
panics — `SamClient::read_line` and `wait_for_datagram` return
`Option<...>` so the test can record evidence keys for every
observed branch instead of aborting the run. The Plan 190
`InboundGatewayRoute` + `reply_path_for_inbound_route` adapter
remain unchanged; only the test driver gained boundary-E
robustness.

The Plan 191 `run-destination.sh` invocation lands the
following command-derived evidence:

```text
daemon-strict-profile               true
reference-routerinfo-verified       true
reference-bootstrap-store           1
reference-floodfill-capable         true
i2pr-routerinfo-len                 651
session-established                 1
sam-destination-created             dest_len=391
outbound-build-emitted              true
inbound-build-emitted               true
gateway-frame-detail                tunnel=38146 inner=11
install-pump-summary                installed_ob=1 installed_ib=1 kind_reply=0 kind_other_build=9 msgid_match=1
outbound-installed                  true
inbound-installed                   true
destination-material-real           outbound_slots=1 inbound_slots=1 zero_hop=0 receive=38402
inbound-reply-path                  gateway_matches_reference=true gateway_tunnel=38401 local_receive=38402 ids_distinct=true
outbound-lookup-via-tunnel          cells=1
lookup-pump-error                   0
lease-lookup-completed              leases=3 published=… expires=…
ls2-publication-tunnel              cells=1
destination-outbound-delivered      cells=1 payload_len=27
reference-received-timeout          timeout after 45s (Plan 191 inbound-delivery boundary E)
destination-inbound-send-failed     send_status=timeout (Plan 191 inbound-delivery boundary E)
inbound-delivery-boundary-E-stop    i2pd HandleDataMessage parses I2CP-style Data body, i2pr emits raw 16-byte standard I2NP Data body (m6 i2cp-wire-format-corrective)
direct-rejected                     true
liveness-first-test                 passed
shutdown-baseline                   true

external-lease-lookup-tunnel        = passed (retained from Plan 190)
external-ls2-publication-tunnel     = passed (retained from Plan 190)
external-destination-outbound       = passed (retained from Plan 190)
external-reference-received         = blocked (m6-inbound-delivery-boundary-E; see Plan 191 §6 stop provenance)
external-destination-inbound        = blocked (m6-inbound-delivery-boundary-E; see Plan 191 §6 stop provenance)
external-direct-rejected            = passed (no longer occluded; asserted in unit suite Plan 187 §11)
external-liveness-first-test        = passed (no longer occluded; asserted in unit suite Plan 187 §11)
```

## External-lane disposition

`external-reference-received` and `external-destination-inbound`
flip from `blocked` to `passed` only after Plan 192 closes the
I2CP-style Data body wire-format layer. The four inbound-delivery
rows stay `blocked` until that plan lands. Plan 191 itself does
not flip them; it isolates the wire-format root cause and stops
there. The corrected inner I2NP short-transport format and the
corrected I2CP-style Data body are preserved on the outbound
side via the narrow `compose_outbound_delivery` seam documented
in Plan 192 §1.

## Isolated wire-format root cause (not enumerated in Plan 191 §3)

Two mismatches between i2pr's destination message-plane output
and i2pd's `ClientDestination::HandleDataMessage`
(`/tmp/i2pd-src/libi2pd/Destination.cpp:1192-1236`):

1. **Inner I2NP envelope**: i2pr's
   `compose_outbound_delivery` (`crates/i2pr-client/src/routing.rs`)
   uses `I2npMessage::new_standard` (16-byte standard header)
   inside the ECIES-X25519 Garlic clove. i2pd's
   `ECIESX25519AEADRatchetSession::HandleECIESX25519GarlicClove`
   (`/tmp/i2pd-src/libi2pd/Garlic.cpp:1023-1028`) parses a 9-byte
   short-transport header (`typeID[1] + msgID[4] + expiration[4]`)
   inside ECIES cloves; the ECIES protocol doc in
   `crates/i2pr-proto/src/ecies_payload.rs::GarlicCloveBlock` even
   says `message` is "the 9-byte I2NP short-form message",
   so i2pr's contract violation is documented. i2pd then
   proceeds to `LeaseSetDestination::HandleCloveI2NPMessage`
   (`/tmp/i2pd-src/libi2pd/Destination.cpp:363-392`) with the
   bytes of our standard header's last 7 bytes + `body` parsed
   as a corrupted payload, never reaching the
   `case eI2NPData:` branch with a real Data body.

2. **Data body format**: i2pr's `OutboundRequest::new`
   (`crates/i2pr-client/src/routing.rs`) wraps the raw
   application payload in `I2npBody::Data::OpaqueMessageBody`
   — 27 bytes of plaintext for the canonical probe. i2pd's
   `ClientDestination::HandleDataMessage`
   (`/tmp/i2pd-src/libi2pd/Destination.cpp:1192-1236`)
   interprets the body as an I2CP-style Data header
   (`length[4 BE] + fromPort[2 BE] + toPort[2 BE] + padding[1]
   + protocol[1] + gzip-no-compression-wrapped payload`); the
   first 4 bytes `"plan"` (= `0x706c616e`) overflow
   `length > len - 4` and i2pd would log
   `Destination: Data message length 1886354798 exceeds buffer
   length 27` and return — except the inner 9-byte-format
   defect above means `HandleDataMessage` is never called.

The reference-side evidence is in
`/tmp/i2pd.log` of the Plan 191 run: i2pd processes the ECIES
session successfully (logs `Garlic: Block type 11 of size 154`
followed by `Garlic: Type local` — the ECIES path is reached)
but never reaches the `Data message length ... exceeds buffer
length ...` log because the inner envelope format defect means
the inner typeID never reads as `eI2NPData = 20`. The fix
belongs to Plan 192; Plan 191 stops at this point per §6.

## Stop conditions

Plan 191 §6 fired: the corrected run reached a fresh
protocol-layer defect not addressed by the typed reply path
(`InboundGatewayRoute`), the gateway-route metadata
(`reply_path_for_inbound_route`), or the destination
message-plane seams. The passing reply-path correction is
preserved; only the inbound-delivery layer is escalated to
Plan 192. No `milestone6_interoperable = passed-via-plan191`
claim is made.

## Handoff

Plan 192 is the next executable plan. If it passes, the deferred
`plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming
pass becomes executable; Plan 189 (Java I2P second-family
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
