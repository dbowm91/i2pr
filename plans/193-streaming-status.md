# Plan 193 status — M6 i2pd mixed-router Streaming qualification (execution log)

Status: **direction-A-qualified** (local rows passed; Direction A
external lane passed end-to-end on exact-head `2f3e510`; Direction
B / close / siblings / reverse-data external rows not yet run).

Plan of record:
[`plans/193-m6-i2pd-mixed-router-streaming-qualification.md`](193-m6-i2pd-mixed-router-streaming-qualification.md).

This status supersedes the historical
`plans/188-streaming-status.md` for execution purposes; the
historical file is kept only as evidence provenance. The new file
is named `plans/193-streaming-status.md` to match the executable
plan number registered in `plans/193-status.md`.

## Current authority

```text
streaming_local_unit = passed (15 rows, streaming_tunnel_unit)
streaming_local_live = passed (11 rows, streaming_tunnel_live)
streaming_external_driver = passed-direction-A
  (streaming_tunnel_external::streaming_through_i2pd, ignored-gated;
  exact-head 2f3e510 lane green: SYN-accepted + Established +
  25 B digest + 8192 B digest, i2pr -> i2pd)
streaming_external_lane = direction-A-passed
  (run-streaming.sh 22/22 rows green incl. workspace-gates slice;
  evidence.json m6_streaming = passed-via-i2pd-2.61.0 on 2f3e510)
streaming_external_remaining = direction-B-establish,
  reverse-data-digest, close-half-close, siblings
  (not yet executed; driver ends after multipacket + baselines)
m6_streaming_i2pd = direction-A-qualified (NOT passed-via-plan193;
  full §14 matrix requires the remaining rows)
m6_second_family_java = not-yet-started (Plan 194, blocked until this pass closes)
milestone6_interoperable = not-yet-claimed
next_step = extend the external driver (Direction B / reverse data /
  close / siblings) or record explicit §14.9 justification, then
  re-run the lane for the full matrix
```

## Source floor

Registration source floor:

```text
i2pr main = 05d6d52870a81eda8891dc492d43c6d4b79bbae8
plan_192 = passed-m6-i2cp-wire-format-corrective
routine CI = 34668461179 (success)
workspace = 2283 passed, 6 ignored
```

Plan 193 scaffolding adds 26 local rows (15 unit + 11 live) and one
ignored external driver under the existing Plan 192 inbound-delivery
corrective. The Direction A qualification additionally landed narrow
wire-compatibility correctives (ECIES block types/flags, clove
order + inbound selection, short-transport opaque framing,
general-deflate I2CP decode, per-delivery tunnel message ids);
see Stop provenance below. No SSU2/tunnel/NetDB/LeaseSet2 wire change.

## What landed

Local product (no wire change; existing `i2pr-client::streaming`
core through the existing `StreamingDestinationAdapter` +
Plan 122 destination-routing pipeline):

```text
crates/i2pr-daemon/tests/streaming_tunnel_unit.rs (new, 15 rows)
  adapter ceiling, client-payload round-trip, i2cp body per-protocol,
  9-byte short-transport envelope, codec errors, SYN flag peek,
  signature install/preimage
crates/i2pr-daemon/tests/streaming_tunnel_live.rs (new, 11 rows)
  SYN round-trip, data round-trip, multi-packet, CLOSE, 2-stream
  isolation, oversized reject, adapter pipeline, port-tuple,
  inbound-chain decode (i2pr<->i2pr through real destination tunnels
  via the local_delivery seam)
```

External lane (fail-closed, loopback-only, exact-pinned i2pd 2.61.0
`635b013a612ff47278ef02acf8580a28e10e26c5`):

```text
crates/i2pr-daemon/tests/streaming_tunnel_external.rs (new)
  streaming_through_i2pd (#[ignore], explicit --ignored --exact only;
  missing env fails closed): strict SSU2 profile -> RI verify ->
  floodfill -> session -> SAM STREAM destination -> real one-hop
  outbound/inbound installs -> tunnel NetDB LS2 lookup -> local LS2
  publication -> Direction A SYN via ECIES/Garlic + real outbound
  tunnel -> SYN-ACK via real inbound tunnel -> Established ->
  small-data digest via SAM ACCEPT socket -> 8 KiB multipacket
  digest -> direct-rejected -> liveness-first-test. Stop paths
  record `streaming-stop` + direct-rejected + liveness-first-test
  and fail closed; rows the stop precedes stay `blocked`, never
  `passed`.
tests/integration/m6-interop/run-streaming.sh (new)
  local suites + explicit ignored driver + sanitized reference facts
  + 22-row evidence.json/evidence.md (digests/lengths/counters only)
scripts/check-streaming-tunnel-evidence.sh (new)
  22 guarded labels wired through m6_row/m6_key_row/ref_row/
  blocked_row/record_guarded only; rejects literal pass records
```

The Plan 189 §8 cross-family aggregator (`run-m6-mixed-router.sh`)
already binds the i2pd first-family streaming rows through
`cross_family_row`; the Plan 194 second-family Java rows stay
recorded `failed` with stop provenance until a follow-up plan
lands the Java qualification harness. No M6 wire change.

## Executed evidence (exact-head 2f3e510)

```text
bash tests/integration/m6-interop/run-streaming.sh
# LANE_EXIT=0; evidence.json m6_streaming = passed-via-i2pd-2.61.0
# 22/22 rows passed (local suites + Direction A external matrix +
# workspace-gates slice); evidence bound to 2f3e510
cargo test --locked -p i2pr-daemon --test streaming_tunnel_unit -- --test-threads=1
# 15 passed
cargo test --locked -p i2pr-daemon --test streaming_tunnel_live -- --test-threads=1
# 11 passed
cargo test --locked -p i2pr-daemon --test streaming_tunnel_external -- --test-threads=1
# 0 passed, 1 ignored (fail-closed ordinary invocation)
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
# 32 passed (Plan 187/188/190/191 local regression green)
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
# 9 passed
cargo test --locked -p i2pr-daemon --test netdb_tunnel_unit -- --test-threads=1
# 22 passed
cargo test --locked -p i2pr-daemon --test netdb_tunnel_live -- --test-threads=1
# 9 passed
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
# 11 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
# 7 passed
cargo test --locked -p i2pr-daemon --test destination_tunnel_external \
  destination_message_plane_against_i2pd -- --ignored --exact --test-threads=1
# ok (Plan 192 destination lane re-verified against the same fixes)
cargo fmt --all --check
# clean
cargo clippy --locked -p i2pr-proto --all-targets -- -D warnings
# clean
cargo clippy --locked -p i2pr-client --all-targets -- -D warnings
# clean
cargo clippy --locked -p i2pr-daemon --test streaming_tunnel_external \
  --test destination_tunnel_external -- -D warnings
# clean
bash scripts/check-streaming-tunnel-evidence.sh
# passed (22 guarded labels, helpers wired)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# passed (11 guarded labels, two-family pins verified)
bash scripts/check-destination-tunnel-evidence.sh
# passed (21 guarded labels)
bash scripts/check-dependency-direction.sh + check-runtime-boundaries.sh
# ok
```

Direction A external rows proven (exact-pinned i2pd 2.61.0
`635b013a612ff47278ef02acf8580a28e10e26c5`, loopback-only):
SYN-accepted + Established + 25 B digest (`6f574c...c60d68`)
+ 8192 B / 8-fragment digest (`25df24...dacb2f80`) i2pr -> i2pd
through real one-hop tunnels, with the reference `Incoming
stream` + SYN-ACK + QuickAck + SAM ACCEPT delivery on its side.
No `external-*` row beyond Direction A is claimed (passed,
blocked, or otherwise) in this status.

## Stop provenance (all hit and corrected during this lane)

1. `SYN-ACK never established` with `dispatch_outcome =
   Rejected(Session(Ecies(AuthenticationFailed)))` on early runs:
   ECIES block types (`1`/`2`, early-draft) rejected by i2pd, which
   implements the published spec (`11`/`5`, `Session ID` = 1).
   Corrected in `ecies_payload.rs` (block types + `0x00`/`0x20`
   delivery flags + `& 0xE0` decode).
2. `Adapter(NotI2npData)` in `streaming_tunnel_live` (8 rows) after
   the LS2-first clove reorder: `decode_cloves` took the raw first
   clove (now the DatabaseStore) as the application clove.
   Corrected in `dispatch.rs` (first non-LS2 clove wins).
3. `Destination: Data: Unexpected protocol` (i2pd side) with the
   source-port low byte as the protocol: short-transport
   `Data`/`Garlic` bodies carried an extra `u32` length word the
   clove parser does not expect (4-byte shift). Corrected in
   `i2np/message.rs` (raw opaque bodies in short-transport form).
4. `I2cpDataBody(BadStoredBlockHeader)` on the SYN-ACK: i2pd
   compresses streaming payloads with real deflate; the decoder
   only accepted stored blocks. Corrected in `i2cp_data_body.rs`
   (`flate2` inflate + bounded output + CRC/ISIZE checks; no new
   dependency).
5. `PortTupleMismatch (0, 0)`: the reference answers from its
   default `StreamingDestination` (`m_LocalPort = 0`) with the
   inbound stream port (`m_Port = 0`), so its SYN-ACK envelope is
   `(fromPort = 0, toPort = 0)`. Lane-scoped to the default-port
   tuple (no product change; nonzero tuples stay proven locally).
6. `Socket already in use` on `STREAM ACCEPT`: the
   session-creation socket is bound to the STREAM session, so
   ACCEPT needs a fresh socket (`SAM.cpp::ProcessStreamAccept`).
   Driver opens a dedicated acceptor socket.
7. Multipacket stall (8 fragments, 0 observed): every composed
   delivery reused tunnel message id `1`, and the driver reseeded
   its RNG per call, so burst cells collided at i2pd's
   msgId-keyed reassembler. Production fix: fresh random nonzero
   id per delivery (`routing.rs`); driver fix: one advancing RNG
   instance per lane.
8. `external-streaming-reference-accepted` row unconditionally
   red: the lane counted an i2pd log line that does not exist in
   2.61.0. Corrected `run-streaming.sh` to count the emitted
   `Streaming: Incoming stream from ` line.

## Next

1. Extend the external driver with Direction B (i2pd initiator ->
   i2pr wildcard-0 listener/accept), reverse-direction data, and
   close/half-close + sibling rows for the full §8 matrix (or
   record the explicit §14.9 justification for retained local
   equivalents).
2. Re-run `bash tests/integration/m6-interop/run-streaming.sh`
   for the full matrix on the new head.
3. On green i2pd family: close Plan 193 per `plans/193-status.md`
   (`plan_193 = passed-m6-i2pd-mixed-router-streaming`,
   `next_executable_plan = 194`) and unblock Plan 194 (Java
   second-family qualification); M10 remote service interop
   stays open until then.
