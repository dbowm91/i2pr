# Plan 152 — narrow Milestone 6 session/streaming robustness corrective

Status: **passed-m6-session-streaming-robustness-corrective (closed
2026-09-03 with Plan 151; full workspace floor and hosted lanes green
on the closing head)**.

Registered: **2026-09-03**.

Plan 151 §17 stop condition triggered this plan: Plan 151's new
executable acceptance tests exposed three genuine Milestone 6
session/streaming defects. Per the stop rule this plan corrects them
narrowly instead of weakening any Plan 151 expectation.

This plan changes **no wire semantics**: same Streaming packets,
same ECIES wire forms, same flag/option/payload encoding. It changes
only local resource policy (what is retained, when ACKs are emitted,
when ratchet keys are trimmed).

## Defects (all proven by executable tests before any fix)

### D1 — unbounded receiver retention under a stalled reader

`StreamingManager::pending_delivered` grows with offered load when
the application pump stalls. Received bytes are ACKed by the
per-destination driver independently of the pump drain, so the
sender's window keeps sliding and the receiver accumulates the
entire offered payload in memory. Code path:
`handle_data_packet` → `pending_delivered.push_back` with no cap,
while `poll_acks` keeps acknowledging.

Proven by: `plan151_slow_reader_stays_bounded_and_recovers` /
`plan151_slow_writer_reverse_pressure_recovers`
(`crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs`) —
retained bytes must stay within explicit reservoirs derived from
`StreamingConfig::balanced`, and the writer must stall rather than
buffer unboundedly.

### D2 — duplicate DATA never re-ACKs, stranding the send window

`handle_data_packet` explicitly schedules no ACK work for
duplicates ("duplicates do not extend any deadline"). After a single
standalone-ACK loss, the sender's retransmits are deduplicated
silently, no ACK ever returns, and the unacknowledged send-window
slots leak permanently (until close). Repeated incidents wedge the
window.

Proven by: `plan151_fault_ack_drop_recovers_without_loop` — sender
`tracked_retransmits`/`unacked` never clear on the forward path
alone (masked only if reverse traffic piggybacks the ACK).

### D3 — sender ECIES ratchet keys retained until the session dies

`seal_existing_session` derives each message key through
`EciesTagSet::symm_key`, which retains every derived key and fails
with `TagSetIndexBeyondCeiling` once the index reaches
`MAX_TAG_SET_RETAINED_KEYS (4096)`. Only the receive path trims
(`trim_keys_below` on open); the seal-only outbound set never
trims, so any destination session dies after ~4096 sealed messages
(`Delivery(Send(Session(Protocol("ECIES primitive error"))))` →
typed `delivery_failed` → connection termination → stream EOF).

Proven by: 6-stream bulk probe (streams EOF together at ~4100
seals with `delivery_failed` spiking and no other counter moving).

## Fixes (all local policy, no wire change)

### F-D3 — trim sender ratchet keys after sealing

After a successful `seal_existing_session` on the outbound
(seal-only) tag set, trim keys below the sealed index. Sent keys
are never re-opened by the sender, so retention is pure waste; the
receiver-side look-ahead/trim behavior is untouched. Steady-state
sender retention drops to ~1 key and the 4096 ceiling never trips
in practice. An absolute `MAX_TAG_SET_INDEX` (65535) guard turns a
would-be silent ratchet overflow into a typed error instead of
unbounded growth. Landed in `crates/i2pr-crypto/src/ecies.rs`;
proven by
`seal_existing_session_trims_sender_keys_past_retention_ceiling`.

### F-D1 — per-connection delivered-bytes cap with ACK gating

- Cap undrained delivered bytes per connection at
  `max_recv_window_packets × MAX_PACKET_PAYLOAD_BYTES` (balanced:
  64 × 1730 ≈ 110 KiB), derived from the existing config (no new
  config surface, no wire change). Accounting lives in
  `pending_bytes_by_connection`, incremented on every in-order
  delivery (including reorder-buffer drains) and decremented on
  every pump drain; `remove_connection` purges both the queue slice
  and the accounting so close/RESET cannot leak the gauge.
- While a connection is over cap, its standalone ACK emission is
  snoozed (deadline re-armed +750 ms, bounded timer-driven, no busy
  loop) and its piggybacked DATA carries `FLAG_NO_ACK` so the
  `ackThrough` view cannot slide the peer window; the pending
  standalone deadline is preserved (not cleared) by the NO_ACK
  piggyback. CLOSE/RESET control keeps fresh ACK views (terminal
  packets are rare and bounded).
- After the pump drains below cap, the next snoozed deadline (≤750
  ms) emits the catch-up ACK; the sender slides and converges with
  exact bytes. No pump→manager signal is added.
- The sibling-isolation companion (`drain_delivered_for`, Plan 151
  row 2) keeps per-stream drains from discarding a sibling's bytes.

Effect: the sender's window fills, the raw pump parks in its
bounded `carry`, TCP backpressure stalls the writer — memory stays
bounded at every layer and exact recovery follows resume.

### F-D2 — duplicate DATA schedules a coalesced immediate ACK

When an established connection observes a duplicate (already
delivered) DATA packet, set its pending-ACK deadline to now
(coalesced through the existing single deadline per connection).
ACK-only packets still never schedule ACKs (existing
`plain_ack_form` rule), so no ACK-of-ACK loop is possible, and
retransmit remains RTO-driven (no aggressive churn).

## Non-goals

- No Streaming wire-format change (Plan 128/134 semantics intact).
- No ECIES wire change (Plan 126 vectors intact).
- No delayed-ACK interval change (750 ms reference behavior kept).
- No RTO/backoff change.
- No new tasks, channels, timers, or dependencies.
- No SAM-architecture change (Plan 149 product path untouched).

## Acceptance

1. `plan151_slow_reader_stays_bounded_and_recovers` passes with
   gauges ≤ explicit ceilings and exact 12 MiB (6 × 2 MiB)
   recovery. The profile is deliberately small per Plan 151 §7
   ("the purpose is not throughput benchmarking ... use a
   deliberately small test profile"); 12 MiB still exceeds the
   combined Streaming windows (6 × 110 KiB) and kernel buffers, so
   writers genuinely stall. **PASS** 2026-09-03
   (`cargo test --locked -p i2pr-daemon --test
   sam_stream_final_acceptance plan151_slow_reader --
   --test-threads=1`, ~117 s, zero typed sweep failures).
2. `plan151_slow_writer_reverse_pressure_recovers` passes ditto.
   **PASS** 2026-09-03 (same command shape, ~118 s, zero typed
   sweep failures).
3. `plan151_fault_ack_drop_recovers_without_loop` passes with zero
   stranded sender state on the forward path alone. **PASS**
   2026-09-03 (~5.5 s; failed before F-D2 with "sender state did
   not clear after ACK recovery", proving D2).
4. New manager-level unit tests prove the cap/trim/duplicate-ACK
   behavior deterministically without sockets:
   `duplicate_data_schedules_immediate_coalesced_ack`,
   `over_cap_connection_snoozes_standalone_ack_and_recovers_on_drain`,
   `gated_send_data_carries_no_ack_and_keeps_deadline`,
   `remove_connection_purges_undrained_accounting`,
   `drain_delivered_for_keeps_sibling_bytes_in_order` (plus the
   ECIES `seal_existing_session_trims_sender_keys_past_retention_ceiling`).
   **PASS** 2026-09-03
   (`cargo test --locked -p i2pr-client --lib streaming::manager::tests`,
   6 passed; full `-p i2pr-client --all-targets` green, 14 suites).
5. A 6-stream × 2 MiB bulk run completes with zero `delivery_failed`
   (D3 regression: ~7300+ seals per destination-pair direction,
   beyond the old 4096-seal ceiling). Covered by rows 1–2, whose
   drain helper asserts zero cumulative typed sweep failures on
   both destinations. **PASS** 2026-09-03.
6. Full workspace floor green (Plan 151 §16). **PASS** — full floor
   green on the closing head (fmt, check, workspace tests, clippy
   `-D warnings`, doc, doctests, boundary scripts, evidence
   checker, ntcp2 harness, deny) plus routine CI success on ubuntu
   and macos.

## Stop conditions

If any fix requires changing wire bytes, handshake state
transitions, or the delayed-ACK/RTO schedule, stop and narrow
further rather than smuggling a protocol change inside a resource
fix.

## Observations (not defects, not fixed here)

- Bulk throughput on a debug single-threaded runtime is
  ~130 KiB/s end to end. The per-packet `deliver_outbound` path
  re-validates the peer LeaseSet2 (`ValidatedLeaseSet2::
  from_lease_set2`, i.e. signature verification per packet) and
  rebuilds the inbound tunnel per packet
  (`crates/i2pr-daemon/src/sam/raw_stream.rs`). That cost is
  correct but wasteful; caching the validated peer material is a
  Milestone 8 hardening candidate. It is out of scope here because
  Plan 151 §7 explicitly blesses a small profile and the 12 MiB
  rows pass with wide margin.
- The slow-drain diagnosis also confirmed the post-close session
  teardown (`finish_raw_stream` last-release → `teardown_session`)
  is silent and total: after all attachments close, gauge/counter
  entries vanish. Tests must read post-close baselines tolerantly
  (missing bridge = zero), which the acceptance helper now does.

## Handoff

Implement F-D3, F-D1, F-D2 under this plan, then return to Plan 151
for final acceptance. Milestone 8 remains blocked until Plan 151
passes.
