# Plan 192 — M6 i2pd-compatible I2CP-style Data body wire-format corrective (Plan 191 §6 stop)

Status: **registered executable corrective**. Plan 191 ran the
corrected `run-destination.sh` against exact-pinned i2pd 2.61.0,
documented the inbound-delivery boundary E stop, and recorded the
two inbound-delivery rows (`external-reference-received`,
`external-destination-inbound`) as `blocked` with stop provenance.
This plan owns the narrow protocol-layer defect Plan 191 surfaced:
i2pd's `ClientDestination::HandleDataMessage` parses an I2CP-style
Data header + gzip-wrapped datagram payload, but i2pr's
`compose_outbound_delivery` emits a raw 16-byte-standard I2NP Data
body whose first four bytes are misread by i2pd as the length
field and overflow the available buffer, so i2pd's SAM bridge
never observes a `DATAGRAM RECEIVED` line.

## 1. Goal

Land the narrow protocol-layer corrective that lets the inbound
delivered byte stream that already reaches i2pd's
`ClientDestination::HandleDataMessage` actually reach the i2pd SAM
bridge as a `DATAGRAM RECEIVED` line, without changing any wire
semantics outside the destination message-plane seam, without
weakening authentication, and without substituting `LocalZeroHop`
or direct transport for any counted row.

Specifically:

1. Inside the ECIES-X25519 Garlic clove, write the inner I2NP
   message in the 9-byte NTCP2/SSU2 short-transport form
   (`typeID[1] + msgID[4] + expiration[4] + payload[N]`) that
   i2pd's `ECIESX25519AEADRatchetSession::HandleECIESX25519GarlicClove`
   parses (`/tmp/i2pd-src/libi2pd/Garlic.cpp:1023-1028`). The
   current `i2pr_client::routing::compose_outbound_delivery`
   uses `I2npMessage::new_standard` (16-byte standard header
   including 8-byte expiration_ms + 2-byte size + 1-byte
   checksum) which i2pd parses as a corrupted payload.
2. Wrap the application payload in the I2CP-style Data body
   format i2pd's `HandleDataMessage` expects
   (`/tmp/i2pd-src/libi2pd/Destination.cpp:1192-1236`):
   `length[4 BE] + fromPort[2 BE] + toPort[2 BE] + padding[1] +
   protocol[1] + gzip-no-compression-wrapped payload`. The
   current `OutboundRequest::new` passes the raw application
   payload straight into `I2npBody::Data::OpaqueMessageBody`.
3. Switch the test SAM session from `STYLE=DATAGRAM` (which
   requires a 384-byte ElGamal/DSA `from` Identity, which our
   ECIES-X25519-only i2pr does not have) to a session style
   whose protocol byte accepts our ECIES-only sender. The
   minimal-change options are:
   - `STYLE=RAW` (protocol byte `PROTOCOL_TYPE_RAW = 0x00`) —
     `HandleRawDatagram` writes the inflated payload straight
     to `SAMSocket::HandleI2PRawDatagramReceive`
     (`/tmp/i2pd-src/libi2pd_client/SAM.cpp:1312-`); OR
   - add `PROTOCOL_TYPE_DATAGRAM3 = 0x07` support in our
     `OutboundRequest` and switch the test to
     `STYLE=DATAGRAM VERSION=3`, which uses the inbound
     destination hash instead of an ElGamal/DSA `from`
     identity (`/tmp/i2pd-src/libi2pd_client/SAM.cpp:377-379`).
4. Wire the new 9-byte-header + I2CP-style Data body path
   through the existing
   `crates/i2pr-daemon/src/destination_tunnels.rs::enqueue_destination_payload`
   so the typed `OutboundRequest` reaches
   `compose_outbound_delivery` in the new shape. The
   `decode_short_transport` path already exists in
   `crates/i2pr-proto/src/i2np/header.rs`; the inbound side can
   reuse it after the corrected outbound arrives.

## 2. Starting authority

Plan 191 closed the inbound-delivery boundary E documentation
with the corrected lane stopping at:

```text
external-lease-lookup-tunnel     = passed (retained from Plan 190)
external-ls2-publication-tunnel  = passed (retained from Plan 190)
external-destination-outbound    = passed (retained from Plan 190)
external-reference-received      = blocked (Plan 191 §6 stop; m6 i2cp-wire-format-corrective)
external-destination-inbound     = blocked (Plan 191 §6 stop; m6 i2cp-wire-format-corrective)
external-direct-rejected         = passed (no longer occluded)
external-liveness-first-test     = passed (no longer occluded)
```

The i2pd side already accepts builds, decrypts the ECIES-X25519
session, parses the Garlic envelope, and dispatches the inner
clove (logs from `/tmp/i2pd.log` of the Plan 191 run show
`Garlic: Block type 11 of size 154` followed by `Garlic: Type
local` — i2pd's ECIES path is reached). The defect is purely in
the inner I2NP envelope format i2pr emits, not in any i2pd-side
handshake or authentication state.

## 3. Constraints

- No authentication weakening, no unsupported crypto, no
  `LocalZeroHop`, no fake LeaseSet, no direct-transport
  substitution for any counted row.
- No production wire-format change beyond the destination
  message-plane seam (`compose_outbound_delivery` /
  `OutboundRequest::new`); reply-path metadata, gateway-route
  metadata, lease-selection, and NetDB lookups stay unchanged.
- Plan 190 `InboundGatewayRoute` + `reply_path_for_inbound_route`
  surfaces stay unchanged.
- Evidence hygiene: no parse-raw-i2pd-log, no
  `LocalZeroHop`-tagged row flipped, no timeout relaxation,
  no fake peer values.
- The corrected inbound test must digest-match the inbound
  payload byte-for-byte against the app payload i2pr sent.

## 4. Acceptance criteria

Plan 192 passes only when:

1. `external-reference-received` flips from `blocked` to
   `passed` via command-derived evidence; i2pd's SAM bridge
   observes a digest-matched `DATAGRAM RECEIVED` (or
   `RAW DATAGRAM RECEIVED`) line on the loopback SAM socket.
2. `external-destination-inbound` flips from `blocked` to
   `passed` via command-derived evidence; i2pr recovers the
   inbound reply on the real inbound destination tunnel and
   decrypts it through the existing ECIES path with the
   destination-payload digest equality asserted.
3. Existing Plan 187 / Plan 188 / Plan 190 / Plan 191 `passed`
   rows remain `passed` after the same run; no regression on
   any prior row.
4. Workspace/static quality floor and exact-head routine CI
   pass.
5. `plans/192-status.md` records exact implementation SHA,
   exact hosted CI run, exact external lane result, and the
   byte-exact inbound payload proof.

## 5. Validation commands

Focused local floor:

```bash
cargo fmt --all --check
cargo check --locked -p i2pr-client --all-targets
cargo check --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
cargo test --locked -p i2pr-proto --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
```

External first-family gate:

```bash
bash tests/integration/m6-interop/run-destination.sh
# expected: external-reference-received and external-destination-inbound
#           flip blocked -> passed; lane exits 0 (full acceptance)
```

Then the repository floor used by current authority:

```bash
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
```

Run the existing static evidence/boundary scripts required by
CI; do not weaken them.

## 6. Stop conditions

Stop and register a narrower follow-up if after the corrected run:

- the inbound payload reaches i2pd but a later protocol step
  fails (Streaming, publication, ECIES ratchet);
- the corrected `STYLE=RAW` / `STYLE=DATAGRAM VERSION=3`
  session interacts with another closed layer that is owned by
  another plan;
- the inbound payload reaches i2pr's ECIES decrypt path but the
  destination-payload digest mismatches because of a separate
  receiver-side bug.

In each case preserve the corrected inner I2NP short-transport
format and the corrected I2CP-style Data body, and escalate
only the newly demonstrated layer.

## 7. Handoff

Plan 192 is the next executable plan. If it passes, the deferred
`plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming
pass becomes executable; Plan 189 Java I2P second-family
qualification depends on Plan 188 + Plan 192 + Streaming.
M10 final acceptance stays open.
