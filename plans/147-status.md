# Plan 147 status — SAM 3.1 dedicated raw STREAM socket driver

Status: **`passed-m7-sam31-dedicated-raw-stream-driver`**.

Registered: **2026-09-01**.

Plan of record:
[`plans/147-m7-sam31-dedicated-raw-stream-driver-corrective.md`](147-m7-sam31-dedicated-raw-stream-driver-corrective.md).

Source audit: Plan 145 (`active-m7-sam31-corrective-roadmap`),
[`plans/145-status.md`](145-status.md).

## Outcome

Plan 147 closes the dedicated SAM 3.1 raw STREAM socket driver that Plan 143
was originally supposed to close but did not. Real localhost SAM TCP sockets
now move arbitrary bidirectional binary application bytes through the full
local destination/Streaming product path under bounded backpressure.

The previous source defects removed:

1. `execute_stream_connect()` no longer updates the SAM attachment to
   `SamStreamState::Established` before the underlying Streaming connection
   reaches `ConnectionState::Established`. The CONNECT path now drives
   `StreamingManager::connect()` through the Plan 129 delivery seam,
   notifies the per-destination runtime driver, and parks on the
   destination's `established_signal` until the canonical
   `StreamingManager` transitions to `Established` or a bounded 20 s
   deadline expires. The STREAM STATUS line is written only after the
   authority state is `Established`; timeout maps to
   `RESULT=TIMEOUT`, and the attachment retains on failure so the
   20 s waiter does not race with a concurrent cleanup.
2. Deterministic `ChaCha8Rng::seed_from_u64(0)` removed from the
   production CONNECT and delivery path. Both `execute_stream_connect()`
   and `deliver_outbound()` now use `rand_core::UnwrapMut(&mut OsRng)`
   so the `CryptoRng + RngCore` bound is satisfied by the OS CSPRNG.
3. Command-mode socket ownership is permanently transferred to a
   supervised raw driver after successful establishment. The former
   `&mut TcpStream` dispatch API now returns an owned
   `ConnectionDisposition` with a `RawTransition(RawStreamHandoff)` that
   carries the entire `TcpStream` plus the post-command bytes drained
   from `LineReader::take_buffered()`. The command loop drops the
   `LineReader` and never parses a subsequent byte as a SAM line; on
   raw transition the loop's `teardown_session` is intentionally
   suppressed so the bridge survives for the raw driver and the
   runtime driver.
4. `STREAM ACCEPT` now completes the real inbound SYN → `accept()` →
   `accept_inbound_syn()` → queue/route SYN response → `Established`
   trajectory before raw handoff. The listener is bound on the bridge's
   receiver-mirror `StreamingManager` (the manager that actually receives
   the inbound SYN through the local seam), not on the `streaming_pools`
   manager. The waiter drives the SYN response through the same
   `bridge_to_peer` seam and parks on the established signal until the
   receiver mirror reaches `Established`.
5. The product TCP ↔ `StreamingManager` loop exists as
   `crates/i2pr-daemon/src/sam/raw_stream.rs::run_raw_stream`. The
   driver owns the `TcpStream`, segments each TCP read to the negotiated
   Streaming payload, calls `send_data()` with bounded admission on the
   correct manager (`streaming` for outbound/CONNECT, `receiver_streaming`
   for inbound/ACCEPT), notifies the runtime driver, and yields to avoid
   starving the single-threaded driver. A 20 ms read timeout prevents
   the TCP `read()` from indefinitely starving the `drain_delivered()`
   path when payloads arrive in both directions simultaneously.
6. The `drain_delivered()` path is direction-aware for the same
   reason. `LocalDeliveryReceiver::deliver` routes non-SYN traffic to
   whichever manager owns the stream id so that data from the peer that
   originated the connection lands on the originator's canonical manager
   while inbound SYN data still lands on the receiver mirror.
7. Per-destination runtime driver `run_destination_driver` exists,
   supervised by `ChildScope`, waking on `outbound_signal` and a
   250 ms ticker, draining both managers' outbound queues through
   `bridge_to_peer`, polling `poll_retransmits`/`poll_acks` on
   `streaming_pools` (and re-draining), and notifying
   `established_signal` whenever any connection in either manager
   reaches `Established`.
8. Same-write `STREAM CONNECT ...\n<raw bytes>` preservation is covered
   by `LineReader::take_buffered()` and the raw handoff's
   `initial_raw_bytes` — the buffer is drained verbatim once, never
   re-validated or re-parsed, and the raw driver emits it before any
   further socket read.

## Acceptance criteria evidence

- `cargo test --locked --workspace` — all suites pass (including the
  `sam_stream_raw_product` lane; the `sam_stream_product` and
  `sam_stream_independent` Plan 143/144 lanes remain green as
  regression, and the i2pr-client `plan129_trajectory` destinations
  remain green).
- `cargo test --locked -p i2pr-daemon --test sam_stream_raw_product`
  (`plan147_dedicated_raw_driver_exchanges_application_bytes`) — two
  real Tokio TCP clients HELLO + `SESSION CREATE STYLE=STREAM` import
  the same base-generated `PUB`/`PRIV` that the daemon loopback
  listener accepted; the test spawns the per-destination runtime
  drivers, issues `STREAM ACCEPT` on B and `STREAM CONNECT` on A in
  parallel while the drivers drive the handshake, observes `STREAM
  STATUS RESULT=OK` on both, then exchanges 1024 bytes (0..255 cycle)
  and 2048 bytes (×7 cycle) simultaneously in both directions and
  asserts byte-exact ordered delivery:
  `received_a == payload_b`, `received_b == payload_a`, with payloads
  segmented by the negotiated Streaming max payload and routed through
  `bridge_to_peer` (observed `payload_len: 1024`, `1730`, `318`).
- `cargo fmt --all --check`, `cargo check --locked --workspace --all-targets`,
  `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`,
  `bash scripts/check-dependency-direction.sh` (`dependency direction: ok`),
  `bash scripts/check-runtime-boundaries.sh` (`runtime boundary checks passed`).

## Code changes

- `crates/i2pr-api/src/sam/line_reader.rs` — new `take_buffered()` that
  drains `self.buf` verbatim for the raw handoff; test
  `take_buffered_preserves_binary_bytes` combined into a single `push`
  so binary payload after a command newline is preserved via
  `append_unchecked` rather than rejected as a control-byte SAM line.
- `crates/i2pr-api/src/sam/server_state.rs` — extended
  `StreamConnectApplied`/`StreamAcceptApplied` with outbound and
  inbound `connection_id`/`peer_destination`, new
  `StreamRawTransition` used by the owning handoff, and
  `LineReader::take_buffered` exposure.
- `crates/i2pr-client/src/streaming/local_delivery.rs` — routing now
  distinguishes initial SYN (always receiver mirror) from SYN response
  (canonical) from established DATA (whichever manager owns the stream
  id via `lookup_outbound`), so `bridge_to_peer` data from an
  inbound-accepted peer lands on the originator's canonical manager.
- `crates/i2pr-client/src/streaming/manager.rs` — exposed
  `lookup_outbound()` used by the delivery routing.
- `crates/i2pr-daemon/src/sam.rs` — refactored dispatch ownership
  (`ConnectionDisposition`, `RawTransitionPayload`), permanent
  `LineReader` detachment, suppressed `teardown_session` on raw
  transition, CONNECT/ACCEPT now wait for `Established`, production
  CSPRNG, `wait_for_established` + `wait_for_accept_established`, and
  `run_destination_driver` supervising `deliver_outbound` + retransmit/
  ACK + established notification.
- `crates/i2pr-daemon/src/sam/raw_stream.rs` (new) — `RawStreamHandoff`,
  `run_raw_stream` (bounded TCP read chunk, direction-aware max-payload
  segmentation, `send_data_segment` with
  `notify_outbound_signal` + `yield_now`, 20 ms read timeout so the
  drain is not starved, and `drain_delivered` on the owning manager),
  `send_data_segment`, `deliver_outbound` (drains both managers,
  looks up peer bridge by destination hash, `bridge_to_peer` over the
  local seam, deterministic inbound-tunnel factory per bridge).
- `crates/i2pr-daemon/src/sam/streams.rs` — `bridge_to_peer` now
  records the inbound observation and preserves both managers' state
  across the swap-replace seam; `debug_ids()` for diagnostics.
- `crates/i2pr-daemon/Cargo.toml` — `rand_core` `os_rng` feature for
  `OsRng::TryCryptoRng`.
- `crates/i2pr-daemon/tests/sam_stream_raw_product.rs` (new) — canonical
  localhost byte-product lane described above.

## Documentation corrections

- `README.md` — Milestone 7 status now records Plan 147 as
  `passed-m7-sam31-dedicated-raw-stream-driver` and advances the next
  executable to Plan 148.
- `AGENTS.md` — same plan-status, next-executable, i2pr-api/daemon
  deep-dive, focused lane index, and protocol-support paragraphs
  updated.
- `plans/README.md` — Plan 147 row marked passed with status-record
  pointer.

## What Plan 147 does **not** close

- Two-independent-client final Milestone 7 closure remains Plan 148.
- FORWARD/naming byte-path revalidation remains Plan 148.
- SAM `advertised = false` unchanged; no new protocol surface is
  advertised.
- Slow-reader/slow-writer, loss/duplicate/reorder, close/reset,
  sibling-stream isolation, and multi-megabyte backpressure revalidation
  at the SAM socket boundary remain Plan 148 follow-up.

## Handoff instruction

The next implementation model should read Plan 145 and execute
**Plan 148 only**. Plan 147's byte-product lane is closed as
`passed-m7-sam31-dedicated-raw-stream-driver` and must not be
re-opened without a concrete defect against the recorded evidence.
