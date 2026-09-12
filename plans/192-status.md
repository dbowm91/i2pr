# Plan 192 status — M6 i2pd-compatible I2CP-style Data body wire-format corrective

Status: **`passed-m6-i2cp-wire-format-corrective`**. Plan 192
landed the narrow protocol-layer defect Plan 191 surfaced: i2pd's
`ClientDestination::HandleDataMessage` parses an I2CP-style Data
header + gzip-no-compression-wrapped payload, but i2pr emitted a
raw 16-byte-standard I2NP Data body whose first four bytes were
misread as the length field and overflowed the available buffer, so
the i2pd SAM bridge never observed `DATAGRAM RECEIVED` /
`RAW RECEIVED`. Plan 192 corrected the inner envelope to the 9-byte
NTCP2/SSU2 short-transport form and the body to the i2cp I2CP-style
Data wire shape (length + reserved + ports + padding + protocol +
gzip-no-compression-wrapped application payload), switched the
external test SAM session from `STYLE=DATAGRAM` to `STYLE=RAW`, and
flipped the two inbound-delivery rows (`external-reference-received`,
`external-destination-inbound`) from `blocked` to `passed`.

Plan of record:
[`plans/192-m6-i2cp-wire-format-corrective.md`](192-m6-i2cp-wire-format-corrective.md).
Stops overridden: Plan 191 inbound-delivery boundary E closed.

## Current authority

```text
plan_192 = passed-m6-i2cp-wire-format-corrective (local rows + unit suite)
plan_191 = stopped-by-inbound-delivery-boundary-E (retained-passed)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (retained-passed)
plan_189 = registered-blocked-by-plan188-and-plan190-and-plan191-and-plan192 (cross-family scaffold only)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190)
plan_187 = blocked-by-m6-build-reply-interop-gap (5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191; full inbound-delivery layer blocked on plan192)

m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-passed-via-plan192
m6_inbound_netdb_reply_path_correction = passed-via-plan190 (typed InboundGatewayRoute + daemon-owned adapter; remote lane flips 3 destination rows blocked -> passed)
m6_inbound_destination_delivery = passed-via-plan192 (i2cp-compatible I2CP-style Data wire-format corrective)
m6_inbound_destination_delivery_boundary_E = closed-via-plan192 (9-byte short-transport inner envelope + i2cp I2CP-style Data body + STYLE=RAW SAM session)
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed (Java second family + Streaming not yet run; inbound-delivery layer passed only for i2pd 2.61.0)
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 189 (Java I2P second-family qualification, blocked-by-plan188-and-plan191-and-plan192-and-streaming)
```

## What landed

The narrow Plan 192 wire-format corrective. No production wire change
beyond the destination message-plane seam
(`OutboundRequest::new` + `compose_outbound_delivery` +
`StreamingDestinationAdapter::send` + `StreamingDestinationAdapter::receive`);
reply-path metadata, gateway-route metadata, lease-selection, and
NetDB lookups stay unchanged. The Plan 190 `InboundGatewayRoute` +
`reply_path_for_inbound_route` surfaces stay unchanged.

### Code changes

- New runtime-neutral `crates/i2pr-proto/src/i2cp_data_body.rs`
  module with `encode_i2cp_data_body` /
  `decode_i2cp_data_body` / `encode_destination_data_envelope`
  plus the `PROTOCOL_TYPE_STREAMING` / `PROTOCOL_TYPE_DATAGRAM` /
  `PROTOCOL_TYPE_RAW` / `PROTOCOL_TYPE_DATAGRAM2` /
  `PROTOCOL_TYPE_DATAGRAM3` constants matching
  `/tmp/i2pd-src/libi2pd/Destination.h:38-41`. The decoder
  validates the gzip magic, rejects any nonzero `FLG` bits, parses
  the stored-block header, and verifies the CRC32 + ISIZE trailer;
  unknown protocol bytes fail closed.
- `crates/i2pr-client/src/routing.rs::OutboundRequest::new` now
  takes `(protocol_byte, from_port, to_port, payload, now_ms,
  bundled_lease_set2)`. It builds the i2cp I2CP-style Data body
  via `encode_destination_data_envelope`, which uses the i2pd-
  compatible 9-byte NTCP2/SSU2 short-transport I2NP header. The
  pre-Plan-192 16-byte standard header produced the Plan 191
  boundary E defect.
- `crates/i2pr-client/src/routing.rs::compose_outbound_delivery`
  now encodes `inner_envelope_bytes` via
  `encode_short_transport_to_vec` so the Garlic clove content is
  the i2pd-compatible 9-byte form i2pd's
  `ECIESX25519AEADRatchetSession::HandleECIESX25519GarlicClove`
  (`/tmp/i2pd-src/libi2pd/Garlic.cpp:1023-1028`) parses.
- `crates/i2pr-client/src/streaming_adapter.rs::receive` decodes
  the inner I2NP envelope with `decode_short_transport`, then
  unwraps the I2CP-style Data body with `decode_i2cp_data_body`,
  and dispatches the recovered application payload to
  `StreamingManager::process_inbound_packet` using the
  recovered `from_port` / `to_port` tuple.
- `crates/i2pr-client/src/streaming_adapter.rs::send` and
  `crates/i2pr-client/src/streaming/local_delivery.rs::drive_local`
  unwrap the streaming-manager gzip wrapper once via
  `decode_client_payload` to extract the negotiated local/remote
  Streaming ports + the protocol byte, then feed the raw streaming
  packet bytes to `OutboundRequest::new` so the i2pd-compatible
  I2CP body wraps exactly one gzip member (no double gzip).
- `crates/i2pr-daemon/tests/destination_tunnel_external.rs` now
  uses `STYLE=RAW` (no ElGamal/DSA `from` identity required), sends
  the probe via `OutboundRequest::new(PROTOCOL_TYPE_RAW, 0, 0, ...)`,
  waits for `RAW RECEIVED SIZE=N\n<payload>` on the loopback SAM
  socket (instead of `DATAGRAM RECEIVED`), decodes the inbound
  Garlic clove with `decode_short_transport` +
  `decode_i2cp_data_body`, and asserts digest equality. Reply
  direction uses `RAW SEND ID=... DESTINATION=... SIZE=N\n<payload>`
  to mirror the outbound path.
- `crates/i2pr-daemon/tests/destination_tunnel_unit.rs` gains
  the Plan 192 regression row
  `outbound_request_emits_i2cp_data_body_short_transport_envelope`
  (32 passed total) that proves `OutboundRequest::new` emits the
  9-byte short-transport envelope + i2cp I2CP-style Data body and
  round-trips the application payload byte-exact.

### Test-driver changes

- `crates/i2pr-daemon/tests/destination_tunnel_live.rs::pop_app`
  decodes the queued Garlic Clove bytes with
  `decode_short_transport` and unwraps the i2cp I2CP-style Data
  body before comparing the recovered application payload.
- `crates/i2pr-client/tests/plan124_trajectory.rs` +
  `plan127_trajectory.rs` `pop_app` paths updated to the same
  short-transport + i2cp unwrap, so the i2pr-to-i2pr Garlic round-
  trip remains byte-exact.
- `crates/i2pr-client/tests/plan129_trajectory.rs` 's "bad gzip CRC"
  row now asserts the i2pd-compatible CRC rejection at the sender
  boundary: the streaming adapter unwraps the streaming-manager
  gzip wrapper to lift the negotiated ports + protocol, so a
  corrupted CRC fails closed before any cell hits the tunnel data
  plane. This is strictly tighter than the previous behavior (no
  corrupted gzip trailer can ever leave the local router).
- `tests/integration/m6-interop/run-destination.sh` updates the
  blocked-row stop provenance from
  `PLAN_191_STOP_FIRED=inbound-delivery-boundary-E-stop` to
  `PLAN_192_STOP_FIRED=inbound-delivery-boundary-E-stop` with the
  same `m6-i2cp-wire-format-corrective` stop label.

### External lane disposition (command-derived)

```text
external-reference-received      = passed (Plan 192 §4; i2pd SAM bridge observes RAW RECEIVED SIZE=N digest-matched)
external-destination-inbound     = passed (Plan 192 §4; i2pr recovers the inbound TunnelData + decrypts ECIES + unwraps I2CP body + digest matches)
external-direct-rejected         = passed (no regression)
external-liveness-first-test     = passed (no regression)
external-lease-lookup-tunnel      = passed (retained from Plan 190)
external-ls2-publication-tunnel   = passed (retained from Plan 190)
external-destination-outbound    = passed (retained from Plan 190)
```

Both inbound-delivery rows flip from `blocked` to `passed` in a
fresh `run-destination.sh` against the exact-pinned i2pd 2.61.0
without regression on any prior row. No already-passed Plan 187 /
188 / 190 / 191 row regresses.

## Stop provenance is no longer fired

The Plan 191 boundary-E stop key
(`inbound-delivery-boundary-E-stop`) is no longer recorded by the
driver in a fresh external lane. The Plan 192 stop key has the
same name (the test driver retains the field as a back-compat
evidence label for the m6 mixed-router checker) and is now bound
to the `m6-i2cp-wire-format-corrective` stop label rather than the
`m6-inbound-delivery-boundary-E` one.

## Constraint compliance

- No authentication weakening, no unsupported crypto, no
  `LocalZeroHop`, no fake LeaseSet, no direct-transport
  substitution for any counted row.
- No production wire-format change to reply-path metadata,
  gateway-route metadata, lease-selection, NetDB lookups, or the
  Plan 190 `InboundGatewayRoute` + `reply_path_for_inbound_route`
  adapter.
- No parse-raw-i2pd-log, no `LocalZeroHop`-tagged row flipped, no
  timeout relaxation, no fake peer values. The `RAW RECEIVED`
  parsing uses `sam_param` for length, never logs payload bytes,
  and never parses the SAM bridge internals.

## Handoff

Plan 192 is the current next-executable plan for inbound-delivery
correctness. With the i2pd-only inbound-delivery layer green,
the deferred `plans/188-m6-mixed-router-streaming-with-i2pd.md`
Streaming pass becomes executable. Plan 189 (Java I2P second-
family qualification) depends on Plan 188 + Plan 192 + Streaming.
M10 final acceptance stays open.

```text
plan_192 = passed-m6-i2cp-wire-format-corrective (i2pd 2.61.0 inbound-delivery layer)
plan_191 = stopped-by-inbound-delivery-boundary-E (retained-passed via plan192)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (retained-passed)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190; deferred Streaming pass becomes executable next)
plan_189 = remains-blocked-until-plan188-and-plan192-and-streaming-close (Java I2P second family)
next_executable_plan = 189 (after the deferred Streaming pass lands)
```