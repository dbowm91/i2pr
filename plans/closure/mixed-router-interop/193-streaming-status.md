# Plan 193 status — M6 i2pd mixed-router Streaming qualification (execution log)

Status: **passed-m6-i2pd-mixed-router-streaming** (local rows
passed; full Direction A + Direction B external matrix passed
end-to-end on exact-head `3687189`, two complete passes; §14.9
robustness disposition recorded below).

Plan of record:
[`plans/implementation/mixed-router-interop/193-m6-i2pd-mixed-router-streaming-qualification.md`](../../implementation/mixed-router-interop/193-m6-i2pd-mixed-router-streaming-qualification.md).

This status supersedes the historical
`plans/implementation/mixed-router-interop/188-m6-mixed-router-streaming-with-i2pd.md` for execution purposes; the
historical file is kept only as evidence provenance. The new file
is named `plans/closure/mixed-router-interop/193-streaming-status.md` to match the executable
plan number registered in `plans/closure/mixed-router-interop/193-status.md`.

## Current authority

```text
streaming_local_unit = passed (15 rows, streaming_tunnel_unit)
streaming_local_live = passed (11 rows, streaming_tunnel_live)
streaming_external_driver = passed-full-matrix
  (streaming_tunnel_external::streaming_through_i2pd, ignored-gated;
  exact-head 3687189 lane green twice: Direction A SYN + Established
  + 25 B + 8192 B digests + reverse 23 B + 4096 B digests + sibling
  + close/EOF + isolation; Direction B CONNECT + Established + 17 B
  + 2048 B digests + close/EOF; manager cleanup queued=0 delivered=0)
streaming_external_lane = passed
  (run-streaming.sh 33/33 rows green incl. workspace-gates slice;
  evidence.json m6_streaming = passed-via-i2pd-2.61.0 on 3687189,
  two complete passes)
streaming_external_remaining = none
m6_streaming_i2pd = passed-via-plan193
m6_second_family_java = not-yet-started (Plan 194, now executable)
milestone6_interoperable = not-yet-claimed
next_step = Plan 194 Java I2P second-family qualification
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
the full-matrix closure additionally landed the per-turn
ACK/retransmit pump drain, fresh SAM sockets for ACCEPT/CONNECT,
the `destination_path()` receive bound, and the 4 KiB SAM read
hygiene (see Stop provenance items 9-15). No SSU2/tunnel/NetDB/
LeaseSet2 wire change; the only production-semantic change is the
receive-side acceptance bound (send path unchanged).

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

## Executed evidence (exact-head 3687189, two complete passes)

```text
bash tests/integration/m6-interop/run-streaming.sh  # PASS1=0
bash tests/integration/m6-interop/run-streaming.sh  # PASS2=0
# evidence.json m6_streaming = passed-via-i2pd-2.61.0, 33/33 rows
# passed, evidence bound to 3687189
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
cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1
# 15 passed
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
# 11 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
# 7 passed
cargo test --locked --workspace --all-targets -- --test-threads=1
# 95 suites, 2312 passed, 0 failed, 7 ignored
cargo fmt --all --check
# clean
cargo check --locked --workspace --all-targets
# 0 errors
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# clean
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# clean (one private intra-doc link from the Direction A commit fixed)
cargo test --locked --workspace --doc
# ok
bash scripts/check-streaming-tunnel-evidence.sh
# passed (33 guarded labels, helpers wired)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# passed (11 guarded labels, two-family pins verified)
bash scripts/check-destination-tunnel-evidence.sh + check-netdb-tunnel-evidence.sh
# passed
bash scripts/check-dependency-direction.sh + check-runtime-boundaries.sh
+ check-service-tunnel-boundaries.sh + check-fixture-manifest.sh
+ check-{ntcp2,ssu2,i2cp}-vectors.sh + check-ntcp2-interoperability.sh
+ check-constrained-host-lane-boundary.sh + check-sam-acceptance-evidence.sh
+ check-ssu2-acceptance-evidence.sh + check-i2cp-acceptance-evidence.sh
+ check-service-tunnel-acceptance-evidence.sh
# ok
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# ok
cargo deny check advisories bans sources
# advisories ok, bans ok, sources ok
```

External matrix proven (exact-pinned i2pd 2.61.0
`635b013a612ff47278ef02acf8580a28e10e26c5`, loopback-only):

```text
Direction A i2pr -> i2pd (real one-hop tunnels both ways):
  SYN-accepted + Established
  25 B digest 6f574c...c60d68 (peer_line_len=524)
  8192 B / 8-fragment digest 25df24...dacb2f80
  reverse 23 B digest 79bb58...172c763e
  reverse 4096 B digest b88349...e467b27a7 (live loss-recovery, see §14.9)
  sibling established + 23 B digest 7bc5bc...034ed (own ACCEPT socket)
  close state=Closed eof=true
  sibling-isolated 23 B digest 2d5103...0530b13c
Direction B i2pd -> i2pr (normal wildcard-0 listener/accept):
  established attempts=1
  17 B digest 6f28af...1429a5b139
  reverse 2048 B / 2-fragment digest 3b5bfe...2f83e
  close state=Closed eof=true
Reference-side: transit endpoint + gateway created, LS2 stored x2,
  8 tunnel tests ok, 2 Incoming-stream acceptances
Cleanup: manager connections=3(Closed) queued=0 delivered=0,
  SSU2 pending/active zero, liveness first-test green
```

## Stop provenance (all hit and corrected during this lane)

Direction A stops (retained-passed via commits `7dcfb0b`,
`7abb5c7`, `2f3e510`; see prior handoff):

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

Full-matrix stops (corrected in closing commit `c4aa5f1`):

9. Test-thread stack overflow at first poll: the 64 KiB
   `SamClient` stack read arrays inflated every enclosing future
   state past the 2 MiB test-thread stack (the old driver sat
   just under the limit). Driver hygiene fix: 4 KiB stack reads
   (loopback SAM replays the same bytes with more syscalls; zero
   semantic change).
10. Reverse-multipacket stall (`next_expected=2, highest=4`):
    the external pump never called `poll_acks` /
    `poll_retransmits`, and polled them only after inbound
    arrivals — a quiet reference awaiting our delayed ACK
    deadlocked a pump awaiting its next message. Driver runtime
    fix: `drain_streaming_timers` on every pump turn (commit
    message in `c4aa5f1`; no production change — the manager API
    already owned both polls).
11. `Codec/PayloadOverflow(1812>1730)` x8: the reference emits
    streaming data payloads larger than our 1730-byte
    advertisement. Narrow product corrective (Plan 193 §13, no
    new plan: no wire bytes change, acceptance bound only):
    `StreamingReceiveLimit::destination_path()` bounds receives
    by the I2CP Data body ungzipped-output ceiling (61,440;
    anything bigger cannot physically arrive); send path still
    advertises and enforces 1730. Pinned by
    `destination_path_accepts_reference_sized_payloads`; the
    `default()` bound and all its assertions are unchanged.
12. `SESSION STATUS` (first_token=SESSION) answering `STREAM
    CONNECT`: `SAM.cpp::ProcessStreamConnect` rejects CONNECT on
    the bound session socket. Driver fix: dedicated fresh SAM
    socket for CONNECT (same reason as ACCEPT).
13. STATUS OK racing SYN arrival: i2pd answers CONNECT before the
    SYN traverses the tunnel. Driver fix: bounded post-STATUS
    accept-drain reusing the normal backlog path (no injection).
14. `send B reverse data: PayloadTooLarge (2048 > 1730)`: driver
    bug (correct manager rejection of one oversized send).
    Driver fix: 2x1024 chunks like the forward path.
15. `cargo doc` failure (private intra-doc link from the
    Direction A commit): one-word doc fix in `i2np/message.rs`.

## §14.9 robustness disposition

`drop-data-retransmit`, `reorder-two-packets`, and
`duplicate-packet` were exercised LIVE against the reference:
the 4096 B reverse transfer lost sequence 2 in transit; the
pump's NACK + `poll_retransmits` path recovered it with
byte-exact digest equality; reordered 3,4 buffered then
delivered in order; reference retransmits deduplicated (Plan 152
D2 immediate-ACK path). `reference-disconnect-bounded` is proven
by the both-direction close rows (`Closed` + reference socket
EOF within 15 s). `stalled-reader-bounded` stays covered by the
retained local deterministic suites (Plan 152 over-cap snooze +
`poll_acks` gating rows in `streaming/manager.rs`,
`recv_window.rs`). `clean-resource-baseline` is proven by the
`manager-cleanup` (`queued=0 delivered=0`) + `shutdown-baseline`
+ `liveness-first-test` rows. No packet-manipulating harness was
built and no reference was patched, per §8.

## Next

Plan 193 is closed. Plan 194 (Java I2P second-family
qualification) is executable; M10 remote service interop stays
open until then.
