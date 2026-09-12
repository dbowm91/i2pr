# Plan 193 status — M6 i2pd mixed-router Streaming qualification (scaffolding)

Status: **`in-progress`** (local rows passed; external scaffold
landed; external lane not yet run; static checker wired; CI floor
still green on the Plan 192 closing head).

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
streaming_external_driver = landed-not-yet-run (streaming_tunnel_external::streaming_through_i2pd, ignored-gated)
streaming_external_lane = not-yet-run (run-streaming.sh + check-streaming-tunnel-evidence.sh static-pass)
m6_streaming_i2pd = not-yet-claimed
m6_second_family_java = not-yet-started (Plan 194, blocked until this pass closes)
milestone6_interoperable = not-yet-claimed
next_step = run run-streaming.sh against exact-pinned i2pd 2.61.0
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
corrective; no production wire change.

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

## Executed evidence

```text
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
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
# 7 passed
cargo fmt --all --check
# clean
cargo check --locked --workspace --all-targets
# 0 errors
bash scripts/check-streaming-tunnel-evidence.sh
# passed (22 guarded labels, helpers wired)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# passed (11 guarded labels, two-family pins verified)
bash scripts/check-destination-tunnel-evidence.sh
# passed (21 guarded labels)
bash scripts/check-dependency-direction.sh + check-runtime-boundaries.sh
# ok
```

No external lane run yet: no `external-*` row is claimed
(passed, blocked, or otherwise) in this status. The first
`run-streaming.sh` execution against the exact-pinned reference
will record the initial row dispositions with command/log
provenance.

## Stop provenance

None yet. Plan 193 §13 applies: stop for a narrow corrective if
failure is attributable to a specific Streaming wire semantic
(SYN options, ports, ACK/NACK, CLOSE, retransmission, sequence
arithmetic), preserving a minimized deterministic reproducer and
comparing against reference behavior before changing code. Do not
weaken tests or route around Plan 192.

## Next

1. Run `bash tests/integration/m6-interop/run-streaming.sh`
   with provisioned exact-pinned i2pd; record row dispositions.
2. On Direction A green: attempt Direction B (i2pd initiator ->
   i2pr listener/accept) as a follow-up driver extension.
3. On green i2pd family: register Plan 194 (Java second-family
   qualification) and unblock it; M10 remote service interop
   stays open until then.
