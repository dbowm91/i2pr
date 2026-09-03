# Plan 152 status — narrow Milestone 6 session/streaming robustness corrective

Status: **`passed-m6-session-streaming-robustness-corrective`**.

- Registered: **2026-09-03**.
- Closed: **2026-09-03** (with Plan 151 final acceptance).
- Plan of record:
  [`plans/152-m6-session-streaming-robustness-corrective.md`](152-m6-session-streaming-robustness-corrective.md).
- Authoritative closure record: this file (normalized by Plan 153;
  the narrative remains historical context).

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_149 = passed-m7-sam31-self-composing-local-product-corrective
plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = passed-m7-sam31-final-acceptance-evidence-correction
plan_152 = passed-m6-session-streaming-robustness-corrective

milestone6_local_product = passed-via-plan134-with-plan152-robustness-retained
milestone6_interoperable = not-yet-claimed
milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
router_to_router_interoperability = not-claimed

next_executable_plan = 153
```

Plan 134 remains the local Milestone 6 product authority. Plan 152 is a
later robustness correction retained underneath Plan 151. This status
does not create a new Milestone 6 milestone-closure claim and does not
broaden Plan 134 into a mixed-router interoperability claim.

## Why Plan 151 §17 triggered this corrective

Plan 151 §17 stop condition fired during new executable acceptance
work: the Plan 151 final-acceptance suites exposed three genuine
Milestone 6 session/streaming defects. Per the stop rule they are
corrected narrowly here instead of weakening any Plan 151 expectation.

This plan changes **no wire semantics**: same Streaming packets, same
ECIES wire forms, same flag/option/payload encoding. It changes only
local resource policy (what is retained, when ACKs are emitted, when
ratchet keys are trimmed).

## Defects (all proven by executable tests before any fix)

### D1 — unbounded receiver retention under a stalled reader

`StreamingManager::pending_delivered` grows with offered load when the
application pump stalls. Received bytes are ACKed by the
per-destination driver independently of the pump drain, so the sender's
window keeps sliding and the receiver accumulates the entire offered
payload in memory. Code path: `handle_data_packet` →
`pending_delivered.push_back` with no cap, while `poll_acks` keeps
acknowledging.

Proven by: `plan151_slow_reader_stays_bounded_and_recovers` /
`plan151_slow_writer_reverse_pressure_recovers`
(`crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs`) — retained
bytes must stay within explicit reservoirs derived from
`StreamingConfig::balanced`, and the writer must stall rather than
buffer unboundedly.

### D2 — duplicate DATA never re-ACKs, stranding the send window

`handle_data_packet` explicitly schedules no ACK work for duplicates
("duplicates do not extend any deadline"). After a single
standalone-ACK loss, the sender's retransmits are deduplicated
silently, no ACK ever returns, and the unacknowledged send-window
slots leak permanently (until close). Repeated incidents wedge the
window.

Proven by: `plan151_fault_ack_drop_recovers_without_loop` — sender
`tracked_retransmits`/`unacked` never clear on the forward path alone
(masked only if reverse traffic piggybacks the ACK).

### D3 — sender ECIES ratchet keys retained until the session dies

`seal_existing_session` derives each message key through
`EciesTagSet::symm_key`, which retains every derived key and fails
with `TagSetIndexBeyondCeiling` once the index reaches
`MAX_TAG_SET_RETAINED_KEYS (4096)`. Only the receive path trims
(`trim_keys_below` on open); the seal-only outbound set never trims,
so any destination session dies after ~4096 sealed messages
(`Delivery(Send(Session(Protocol("ECIES primitive error"))))` →
typed `delivery_failed` → connection termination → stream EOF).

Proven by: 6-stream bulk probe (streams EOF together at ~4100 seals
with `delivery_failed` spiking and no other counter moving).

## Fixes (all local policy, no wire change)

### F-D3 — trim sender ratchet keys after sealing

After a successful `seal_existing_session` on the outbound
(seal-only) tag set, trim keys below the sealed index. Sent keys are
never re-opened by the sender, so retention is pure waste; the
receiver-side look-ahead/trim behavior is untouched. Steady-state
sender retention drops to ~1 key and the 4096 ceiling never trips in
practice. An absolute `MAX_TAG_SET_INDEX` (65535) guard turns a
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
  delivery (including reorder-buffer drains) and decremented on every
  pump drain; `remove_connection` purges both the queue slice and the
  accounting so close/RESET cannot leak the gauge.
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

Effect: the sender's window fills, the raw pump parks in its bounded
`carry`, TCP backpressure stalls the writer — memory stays bounded at
every layer and exact recovery follows resume.

### F-D2 — duplicate DATA schedules a coalesced immediate ACK

When an established connection observes a duplicate (already
delivered) DATA packet, set its pending-ACK deadline to now
(coalesced through the existing single deadline per connection).
ACK-only packets still never schedule ACKs (existing
`plain_ack_form` rule), so no ACK-of-ACK loop is possible, and
retransmit remains RTO-driven (no aggressive churn).

## Non-goals (unchanged)

- No Streaming wire-format change (Plan 128/134 semantics intact).
- No ECIES wire change (Plan 126 vectors intact).
- No delayed-ACK interval change (750 ms reference behavior kept).
- No RTO/backoff change.
- No new tasks, channels, timers, or dependencies.
- No SAM-architecture change (Plan 149 product path untouched).

## Executed evidence (2026-09-03, closing head)

Only executed commands count; nothing here is marked passed from prose.

1. `plan151_slow_reader_stays_bounded_and_recovers` passes with gauges
   ≤ explicit ceilings and exact 12 MiB (6 × 2 MiB) recovery. The
   profile is deliberately small per Plan 151 §7 ("the purpose is not
   throughput benchmarking ... use a deliberately small test
   profile"); 12 MiB still exceeds the combined Streaming windows
   (6 × 110 KiB) and kernel buffers, so writers genuinely stall.
   **PASS** 2026-09-03 (`cargo test --locked -p i2pr-daemon --test
   sam_stream_final_acceptance plan151_slow_reader --
   --test-threads=1`, ~117 s, zero typed sweep failures).
2. `plan151_slow_writer_reverse_pressure_recovers` passes ditto.
   **PASS** 2026-09-03 (same command shape, ~118 s, zero typed sweep
   failures).
3. `plan151_fault_ack_drop_recovers_without_loop` passes with zero
   stranded sender state on the forward path alone. **PASS**
   2026-09-03 (~5.5 s; failed before F-D2 with "sender state did not
   clear after ACK recovery", proving D2).
4. New manager-level unit tests prove the cap/trim/duplicate-ACK
   behavior deterministically without sockets:
   `duplicate_data_schedules_immediate_coalesced_ack`,
   `over_cap_connection_snoozes_standalone_ack_and_recovers_on_drain`,
   `gated_send_data_carries_no_ack_and_keeps_deadline`,
   `remove_connection_purges_undrained_accounting`,
   `drain_delivered_for_keeps_sibling_bytes_in_order` (plus the ECIES
   `seal_existing_session_trims_sender_keys_past_retention_ceiling`).
   **PASS** 2026-09-03
   (`cargo test --locked -p i2pr-client --lib streaming::manager::tests`,
   6 passed; full `-p i2pr-client --all-targets` green, 14 suites).
5. A 6-stream × 2 MiB bulk run completes with zero `delivery_failed`
   (D3 regression: ~7300+ seals per destination-pair direction,
   beyond the old 4096-seal ceiling). Covered by rows 1–2, whose
   drain helper asserts zero cumulative typed sweep failures on both
   destinations. **PASS** 2026-09-03.
6. Full workspace floor green (Plan 151 §16). **PASS** — full floor
   green on the closing head (fmt, check, workspace tests, clippy
   `-D warnings`, doc, doctests, boundary scripts, evidence checker,
   ntcp2 harness, deny) plus routine CI success on ubuntu and macos.

## Containing acceptance authority (Plan 151 closing head)

Plan 152 closed with Plan 151 and is retained underneath it. The
containing final-acceptance authority is:

- Plan 151 closing commit:
  `02e47aa69a2574165aadd4c28df1128845eb94ab`
  (`plan151: establish sibling pairs sequentially for deterministic
  pairing`).
- Routine CI on the closing head: run `33788586572`, conclusion
  `success` — Quality ubuntu, Quality macos, MSRV, dependency-policy
  all green.
- SAM external workflow on the closing head: run `33790521635`
  (`workflow_dispatch` of `.github/workflows/sam-external.yml` at
  `main`), conclusion `success`; artifact
  `sam-external-evidence-33790521635` uploaded.
- Hosted evidence (`evidence.json`, lane `github-33790521635`,
  `rustc 1.95.0`): **26/26 rows passed**, zero non-passed rows;
  local rerun on the same head agreed 26/26.

Sanitization: evidence carries commands, exit statuses, commit, lane,
client revisions, byte counts, and counter snapshots only; no PRIV,
seeds, secrets, payloads, or environment dumps.

## Plan 153 re-verification (current head)

During Plan 153 execution (starting SHA
`430de3e213861a91ae13ce31ffcf25556855f580`), the Plan 153 static
gates were re-run locally and passed:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
```

Plan 153 does not require rerunning the several-minute Plan 151
slow-peer suite locally merely to normalize this status file; the
exact-head routine CI and manual SAM external workflow are recorded
in `plans/153-status.md` on the Plan 153 closing commit.

## Handoff

Plan 152 is closed. Do not reopen Milestone 6 wire semantics from
this record. Execute Plan 153 for post-M7 authority/CI hygiene; after
Plan 153 passes, execute Plans 155 → 161 in order under the Plan 154
Milestone 8 roadmap. SAM stays experimental, loopback-only, disabled
by default, and non-advertised. No localhost SAM evidence implies
router-to-router NTCP2/SSU2 or public I2P interoperability.
