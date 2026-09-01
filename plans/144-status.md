# Plan 144 status — SAM 3.1 independent-client validation and final Milestone 7 closure corrective

Status: **`partial-passed-m7-sam31-independent-client-handshake-corrective`**.

Registered: **2026-09-01**.

Plan of record:
[`plans/144-m7-sam31-independent-client-final-closure-corrective.md`](144-m7-sam31-independent-client-final-closure-corrective.md).

Source audit: Plan 141,
`active-m7-sam31-corrective-roadmap`; Plan 143,
`passed-m7-sam31-live-stream-product-bridge-corrective`.

## Outcome

Plan 144 lands two independent advances against the Plan 141
corrective sequence:

1. The Plan 143 mirror/canonical-streaming routing asymmetry is
   fixed; the SAM 3.1 product bridge now drives a **full
   bidirectional** in-process handshake round-trip through the
   runtime-neutral `i2pr_client::deliver` seam.
2. The SAM independent-client provenance table is updated with the
   next two candidates (`i2plib` and `libsam3`) for the
   two-independent-client evidence lane.

### What Plan 144 closed

1. **`canonical_streaming` routing fix.** The Plan 129
   `LocalDeliveryReceiver` gains a new
   `canonical_streaming: Option<&mut StreamingManager>` field. The
   local-seam `deliver()` body peeks the recovered streaming packet
   header (via `i2pr_proto::streaming::peek_streaming_header` after
   unwrapping the I2NP Data body + protocol-6 client payload gzip
   layer). Packets with `FLAG_SYNCHRONIZE` set *and*
   `send_stream_id != 0` *and* `receive_stream_id != 0` are routed
   onto the supplied canonical outbound `StreamingManager`; every
   other streaming packet (inbound SYN, data, CLOSE, RESET) lands
   on the receiver-side mirror. Without this fix a Plan 129 SYN
   response cannot reach the `outbound_by_stream` index on the
   `StreamingManager` that issued the SYN, so no full bidirectional
   handshake was observable. The routing decision uses
   `peek_streaming_header` (16 bytes; flags + NACK count + send /
   receive stream ids) so the receiver-side canonical-streaming
   path is payload-agnostic — only the routing decision depends on
   the header bytes, never the streaming body.

2. **`bridge_to_peer` integration.** `SamDestinationBridge::new`
   keeps its two-streaming-manager split (`streaming` for outbound
   / canonical, `receiver_streaming` for inbound mirror). The
   daemon's `bridge_to_peer` helper swaps **both** the peer's
   receiver-side mirror and the peer's canonical outbound
   `StreamingManager` into the `LocalDeliveryReceiver` before each
   `deliver()` call. After `deliver()` returns, both managers are
   restored to the bridge under the same lock that recorded the
   inbound observation. Bridge diagnostics continue to increment
   exactly once per `bridge_to_peer` invocation.

3. **In-process bidirectional handshake evidence.** A new
   integration test
   `crates/i2pr-daemon/tests/sam_stream_independent.rs` drives two
   cooperating SAM destinations through the same `SamDestinations`
   registry. Bridge A issues `StreamingManager::connect` against
   bridge B; the resulting SYN is drained from A's outbound queue
   and routed through `bridge_to_peer` into B with B's inbound
   tunnel as `peer_inbound_tunnel`. Bridge B observes the inbound
   SYN through its receiver-side mirror and the test drives the
   standard `accept` / `accept_inbound_syn` path. The new
   `StreamingManager::queue_outbound_packet` accessor (used
   because `accept_inbound_syn` returns the SYN response as a
   `TransportSendRequest` value rather than auto-queueing it)
   feeds the request into B's receiver-side outbound queue, which
   the test drains and routes back through `bridge_to_peer` into
   A. The SYN response lands on A's **canonical** outbound
   `StreamingManager` through the new `canonical_streaming`
   routing rule; A's connection transitions from
   `OutboundSynSent` to `Established`. The test asserts:
   - both bridges report
     `ConnectionState::Established` (A on `streaming` /
     canonical, B on `receiver_streaming` / mirror);
   - the round-trip crossed exactly two `bridge_to_peer`
     invocations (one A→B for the SYN, one B→A for the SYN
     response);
   - the inbound observation counters incremented under the
     bridge lock in lockstep with the outbound dispatch counters.

4. **`EstablishedTunnel` clone-seam seam.** A test-only
   `EstablishedTunnelClone` trait in
   `sam_stream_independent.rs` rebuilds an `EstablishedTunnel`
   from its hop vector via `EstablishedHop::with_next` /
   `EstablishedHop::terminal` + `hop.layer_keys().clone()`. The
   seam lives only in test code; production code never needs to
   rebuild an `EstablishedTunnel` because the inbound tunnel
   arrives as a caller argument to `bridge_to_peer`, never from
   the bridge (which would force `EstablishedTunnel: Clone`).

5. **Independent-client provenance table.** The Plan 142
   `tests/integration/sam/README.md` table extends with two new
   pinned candidates:
   - `i2plib` `6edf51cd5d21cc745aa7e23cb98c582144884fa8`
     (`v0.0.14`) — Python, MIT — SAM wire helpers in
     `i2plib/sam.py` confirmed correct (HELLO,
     SESSION CREATE, STREAM CONNECT, STREAM ACCEPT, NAMING LOOKUP,
     DEST GENERATE, STREAM FORWARD). Same client previously used
     for the Base64 alphabet validation; ready as Client A.
   - `libsam3` `e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7`
     (`v1.0.0`) — C, mixed (public-domain + MIT components) —
     `make build` produces `libsam3.a` cleanly; example
     `sam3/streamcs.c` demonstrates STREAM CONNECT+ACCEPT. Ready
     as Client B.
   `txi2p` retains the Plan 142 `import-blocked-by-ometa`
   status; a successor plan owns `txi2p` de-blocking or
   replacement.

### What Plan 144 does **not** close

- **Per-stream TCP↔Streaming raw byte bridge.** The actual
  bidirectional TCP ↔ Streaming connection driver that runs after
  `STREAM STATUS RESULT=OK` is written is **not** landed in Plan 144.
  The `DispatchOutcome::StreamRawMode` arm in the daemon's
  `RequireStreamConnect` dispatch replies OK and returns the
  control-socket state to `UtilityReady`; the underlying TCP
  stream is left in line mode. The per-destination driver task
  that owns the raw TCP stream, feeds incoming bytes into
  `StreamingManager::send_data`, drains the outbound queue through
  `bridge_to_peer` (or the production Runtime equivalent), and
  pumps delivered application bytes back into the TCP socket under
  bounded resource ceilings is deferred to Plan 145 (or successor).
- **Two-independent-client evidence lane.** No independent SAM
  client has been driven against the live loopback listener yet.
  Plan 145 owns the i2plib (Python) driver and the libsam3 (C)
  driver integration against the bound SAM listener *and* the
  Plan 145 per-stream raw byte bridge.
- **Plan 144 §5 binary-byte interoperability.** The Plan 144
  ASCII / NUL / 0x00..0xff / `HELLO VERSION` / multi-packet /
  large-payload byte matrix is deferred — the per-stream raw byte
  bridge is the layer that drives a real
  `tokio::net::TcpStream` so client-side payload bytes can be
  observed at the daemon's egress.
- **Plan 144 §6 SILENT byte-exact checks.** Same gate as §5.
- **Plan 144 §7 multi-stream + lifecycle.** Same gate.
- **Plan 144 §8 Plan 139 FORWARD against the corrected bridge.**
  Same gate; the FORWARD driver task + the corrected byte pump
  must land before FORWARD byte evidence can be re-run.
- **Plan 144 §10 resource / adversarial closure.** Defer to Plan
  145; the per-stream raw byte bridge introduces the slow-reader /
  slow-writer surface.
- **Plan 144 §11 privacy / security closure.** Defer to Plan 145
  so the privacy redacted-log assertions cover the raw byte
  path (not just the line-mode command path).
- **Plan 144 §12 M6 regression floor was run as part of the
  commit gate** (`cargo test --locked --workspace --all-targets` —
  1246 passed across 47 suites). Plan 145 retains the explicit
  Plan 129–134 trajectory re-runs as the plan-145 gate.

### Acceptance criteria evidence

#### 1. Bridge API exposes the canonical-streaming routing fix

`crates/i2pr-client/src/streaming/local_delivery.rs`:
- `LocalDeliveryReceiver` gains
  `canonical_streaming: Option<&'a mut StreamingManager>`.
- `deliver()` body peeks the recovered streaming packet header
  after unwrapping the I2NP `Data` body and the gzip-encoded
  protocol-6 client payload. The peek is via
  `i2pr_proto::streaming::peek_streaming_header`; the decision
  rule is `FLAG_SYNCHRONIZE != 0 && send_stream_id != 0 &&
  receive_stream_id != 0`. Without `canonical_streaming` being
  `Some(...)` the new routing rule is a no-op and the SAM
  bridge's existing Plan 143 behaviour is preserved.

`crates/i2pr-daemon/src/sam/streams.rs`:
- `SamDestinationBridge` exposes both managers
  (`streaming()` / `streaming_mut()` for canonical,
  `receiver_streaming()` / `receiver_streaming_mut()` for
  inbound mirror) and a new
  `SamDestinationHandle::lookup_by_peer_hash` /
  `peer_destination_hash` accessor pair that maintains a
  reverse-indexed `HashMap<[u8; 32], DestinationId>` for inbound
  SYN observations.
- `bridge_to_peer` swaps the peer's `streaming` (canonical)
  *and* the peer's `receiver_streaming` (mirror) into the
  `LocalDeliveryReceiver` before `deliver()`, restores both after.

`crates/i2pr-client/src/streaming/manager.rs`:
- `StreamingManager::queue_outbound_packet(&mut self, request:
  TransportSendRequest)` pushes a single transport-send request
  into the outbound queue. Used by the test seam after
  `accept_inbound_syn` returns the SYN response as a value rather
  than auto-queueing it.

#### 2. Local delivery seam routes by streaming header

The dual-manager routing rule sits behind the canonical
`StreamingDestinationAdapter::receive` entry point; the same
interface accepts both managers unchanged. Plan 143's
`LocalDeliveryReceiver::streaming` field continues to feed the
receiver-side mirror as a fall-through default.

#### 3. In-process bidirectional handshake test

`crates/i2pr-daemon/tests/sam_stream_independent.rs` (NEW):

- `plan144_full_handshake_reaches_bidirectional_established` —
  drives A→B SYN, B's standard `accept` + `accept_inbound_syn`,
  B→A SYN response, and asserts both `ConnectionState::Established`
  states. The test rebuilds the inbound `EstablishedTunnel`
  across `bridge_to_peer` calls via the test-only
  `EstablishedTunnelClone` trait. The bridge-to-peer call
  signature is the canonical Plan 129 form (sender handle, peer
  handle, outbound hop hashes, request, time context, outbound
  tunnel id, peer inbound tunnel, RNG).

#### 4. Workspace / boundary gates

- `cargo fmt --all --check` — passes.
- `cargo check --locked --workspace --all-targets` — passes.
- `cargo test --locked --workspace --all-targets` — 1246
  passed across 47 suites. The new
  `sam_stream_independent` suite contributes one focused test.
- `cargo clippy --locked -p i2pr-client -p i2pr-daemon --all-targets -- -D warnings` — passes.
- `bash scripts/check-dependency-direction.sh` — passes.
- `bash scripts/check-runtime-boundaries.sh` — passes.
- No fixture / NTCP2 vector / interop / rootless / multipass /
  constrained-host-lane files changed in this commit, so the
  corresponding scripts require no re-run.

#### 5. Documentation and provenance

- `README.md` — Plan 144 in-process handshake evidence noted;
  per-stream raw bridge + two-independent-client evidence lane
  deferred to follow-up.
- `docs/architecture/i2pr-client.md` — Plan 144 paragraph added
  after the Plan 143 `bridge_to_peer` paragraph describing the
  `canonical_streaming` routing rule and the new
  `sam_stream_independent.rs` test as the canonical in-process
  evidence.
- `docs/architecture/i2pr-daemon.md` — `src/sam/streams.rs`
  inventory updated to call out the Plan 144 canonical-streaming
  routing fix, `SamDestinationHandle::lookup_by_peer_hash`,
  `receiver_streaming`, and `peer_destination_hash`.
- `tests/integration/sam/README.md` — provenance table extended
  with `i2plib` and `libsam3` entries; Plan 143/144 evidence
  lane updated to record the in-process handshake closure and
  the per-stream raw bridge follow-up.
- `plans/README.md` — Plan 144 row reclassified as
  `partial — passed-m7-sam31-independent-client-handshake-corrective`;
  reference to `plans/144-status.md` added.

### Handoff instruction

The next implementation model should read Plan 141 and execute
**Plan 145** (candidate) — Plan 144's deferred work:
**per-stream TCP↔Streaming raw byte bridge** +
**two-independent-client evidence lane** + **Plan 144 §5–§8 byte
acceptance** + **Plan 144 §10–§11 closure** + **Plan 144 §12 M6
regression floor**.

Plan 144 does **not** re-open Plan 142 or Plan 143; both remain
`passed`. The Plan 144 in-process bidirectional handshake +
canonical-streaming routing fix must not be regressed without a
failing evidence lane.
