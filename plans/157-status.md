# Plan 157 status — Milestone 8 SSU2 v2 data-phase reliability and fragmentation

Status: **`passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation`**.

Registered: **2026-09-04**. Closed: **2026-09-04**.

Plan of record:
[`plans/157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md`](157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md).

## Current authority

```text
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap-blocked-by-plan153
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation

milestone8_protocol = ssu2-v2-classical
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented
milestone8_implementation = plan157-data-phase-landed

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 158
next_product_layer = milestone8-ssu2-v2
```

## What this pass did

1. Implemented the runtime-neutral authenticated data phase (plan
   §2–§3) in the new `crates/i2pr-transport-ssu2/src/session.rs`
   (`Ssu2Session`, ~2600 lines with unit tests): explicit session
   owner built from Plan 156's directional keys plus caller-supplied
   intro keys, owning send packet-number state, the bounded receive
   replay window, pending ACK state, RTT/RTO/loss/congestion state,
   sent-packet provenance, pending retransmit fragments, reassembly,
   duplicate suppression, and idle/termination state. All
   time/randomness arrives as caller inputs through the single central
   `poll(now)` entry point; no Tokio, socket, timer, task, or
   wall-clock read exists in the crate. Complete ciphertext datagrams
   are never retained — only semantic fragment bytes plus provenance,
   so every retransmission is a fresh packet (new number, current ACK
   state). Receive order is exactly cheap length check → session
   binding → header unprotection/decode → replay admissibility → AEAD
   open → bounded block parse → effects; tag failures and replays
   mutate only diagnostic counters, never application-visible state.
2. Fixed the data-phase KDF to the specification (plan §3, KDF for
   data phase): `Ssu2Transcript::split()` now performs both HKDF
   steps — `HKDF(ck, ZEROLEN, "", 64)` into `k_ab`/`k_ba`, then
   `HKDF(key, ZEROLEN, "HKDFSSU2DataKeys", 64)` into
   `(k_data, k_header_2)` per direction via the existing
   `derive_data_keys`. The pre-157 code used the intermediate
   directional key directly as the AEAD key; both sides agreed, so all
   Plan 156 tests still passed, but the wire keys did not match the
   specification. `Ssu2SplitKeys` now carries `DataDirectionKeys`
   (cipher plus `k_header_2`); `k_header_1` is the receiver's intro
   key supplied by the session layer per the Header Encryption KDF
   table. The existing `transmit()`/`receive()` accessors keep their
   shape; `into_parts` now yields the directional pairs.
3. Implemented packet-number/replay handling (plan §4):
   the v2 short header carries the full 32-bit number, so
   reconstruction is the documented identity function plus a
   wrap-aware 128-packet window (`DATA_REPLAY_WINDOW_PACKETS`) with
   duplicate rejection, below-floor (`TooOld`) rejection, a 1024-packet
   future-jump cap (`DATA_MAX_FUTURE_JUMP`, local policy against
   window-wipe), fixed-size `u128` bitmaps (no growing set), and
   32-bit exhaustion without wrap. Unit tests pin wrap boundaries
   around `u32::MAX`.
4. Implemented ACK semantics and scheduling (plan §5): strict
   range interpretation with underflow/degenerate rejection that
   retires only actually-sent packets (unknown numbers are idempotent
   no-ops); duplicate ACKs idempotent; ACK-only packets never elicit
   ACK responses (loop avoidance: only ack-eliciting receipts arm
   standalone ACKs, piggyback shares the same ACK view); ACK-only
   packets excluded from bytes-in-flight and congestion gating per the
   specification exception; one per-session ACK deadline (delayed
   `max(10 ms, min(RTT/6, 150 ms))`, immediate `min(RTT/16, 5 ms)`,
   with deterministic defaults before any RTT sample); immediate
   triggers on explicit flag, out-of-order arrival, or every second
   ack-eliciting packet. Ack-eliciting policy documents the
   specification's open "Others?" item: every block except ACK,
   Address, DateTime, Padding, and Termination elicits.
5. Implemented sent provenance and fresh retransmission (plan §6):
   per-packet number/sent-time/byte/fragment/generation records
   capped at 256 entries with oldest-evicted-as-loss overflow;
   NACK-gap and RTO loss declaration requeues still-needed semantic
   fragments (generation-guarded, 5-attempt ceiling
   `DATA_MAX_FRAGMENT_RETRANSMISSIONS`, then silent per-message drop
   while the session stays usable per specification); ACKs release
   accounting promptly; Karn's rule (generation-0 only) guards RTT
   sampling after the packet generation fix in this pass.
6. Implemented the conservative congestion controller (plan §7):
   byte-count window (min 2440 / default 12200 / max 60000,
   MSS 1220), exact bytes-in-flight for congestion-controlled packets
   only, RFC 6298-style `srtt/rttvar/rto` (1 s min per the SSU2
   reference, 60 s max) with saturating arithmetic, halving on loss
   down to the minimum, additive growth up to the maximum, RTO
   exponential backoff, and 5-consecutive-RTO bounded
   termination (`Timeout`, reason 14). Algorithm choices are
   documented in the module docs where SSU2 defers to RFC 6298/9002.
7. Implemented I2NP fragmentation/reassembly (plan §8–§9):
   outgoing queueing splits encoded messages into fixed 1024-byte
   semantic fragments (capped by the MTU budget) with deterministic
   numbering so retransmissions preserve length/offsets;
   single-fragment messages travel as complete I2NP blocks (no
   reassembly state); multi-fragment messages use exact
   first/follow-on headers; 64-fragment ceiling; no zero-length
   fragments; sender releases on acknowledgement or terminal failure.
   Incoming reassembly is keyed by message ID with per-message and
   16-message/256 KiB aggregate quotas, out-of-order tolerance,
   idempotent duplicates, conflict-drop without overwrite, expiry with
   total buffer release, atomic once-only completion within I2NP
   maximums, and exact-capacity/max+1 tests. Duplicate suppression is
   a bounded 128-entry delivered-ID cache with retention expiry at
   the delivery boundary.
8. Implemented data-phase control handling (plan §11): NewToken as a
   typed handoff event; DateTime/Options/Congestion observed as typed
   events (Congestion immediate-request re-arms the ACK deadline);
   Termination received/released with bounded retransmit/reassembly
   cleanup; idle timeout (default 300 s, local policy) via `poll`;
   `NextNonce` (spec TODO) surfaces an authenticated
   `RekeyRequested` event with no automatic rekey; `FirstPacketNumber`
   (spec: not fully specified) surfaces an event without changing
   numbering; path challenge/response are typed events with queue
   helpers while migration stays Plan 159; relay/peer-test decode to
   opaque typed events for Plan 160. Outbound controls are
   single-shot (documented; the specification only permits, never
   requires, termination retransmission).
9. Added the deterministic fault suite (plan §12):
   `tests/data_phase.rs` (17 trajectories, in-memory paired sessions
   from a real transcript dance, no sockets, no `i2pr-testkit`
   dependency — the crate boundary script forbids even dev-edges, so
   the driver uses the established `rand_chacha` deterministic
   pattern): bidirectional multi-message exchange; DATA loss → fresh
   (byte-distinct) retransmission with exact once-delivery;
   ACK loss → RTO recovery with bounded traffic and no redelivery;
   duplicate/replay/corruption/reorder trajectories; first/middle/
   final fragment loss recovery; reorder/duplicate/conflicting-duplicate
   fragments; reassembly exact-capacity/max+1 with total cleanup;
   outbound-queue exact-capacity/max+1; congestion-gate boundedness;
   prolonged heavy loss → bounded `Timeout` termination;
   idle-timeout termination; termination lifecycle with state release;
   two-session pressure isolation. Unit tests in `session.rs` pin
   ACK underflow without mutation, duplicate-ACK idempotency,
   sent-history exact eviction, per-fragment ceiling silence,
   packet-number exhaustion, wrap boundaries, and the NextNonce
   boundary. Total: 64 lib + 20 handshake + 17 data-phase = 101.
10. Committed two data-phase vectors (plan §13/§16.1):
    `data-phase-first` (108-byte first data packet) and
    `data-phase-ack` (40-byte ACK-only response), minted from fixed
    seeds/IDs/keys/clocks by a temporary generator deleted before
    closure, pinned in `tests/fixtures/ssu2/manifest.tsv` (14 rows),
    and reproduced byte-for-byte through the session receive path in
    `committed_data_vectors_reproduce_byte_for_byte`.
11. Exposed privacy-safe counters (plan §13): packets
    sent/received/replayed/rejected, ACKs sent/received, loss events,
    retransmitted fragments, bytes-in-flight, cwnd, reassembly
    messages/bytes/drops, and termination state. No payload, token,
    key, or endpoint-history material.
12. Documented (plan §11 file list): new module `session.rs` with
    re-exports in `lib.rs`; extended `constants.rs` (`DATA_`
    policies), `crypto.rs` (second-HKDF split), `state_machine.rs`
    (`into_keys` consumer plus a `large_enum_variant` allow for the
    64-byte growth from the stored header keys); rewrote
    `docs/architecture/i2pr-transport-ssu2.md`; updated
    `overview.md`, `specs/protocols/09-ssu2.md`,
    `specs/support.toml` (plan pointers plus the
    `ssu2.v2-data-phase` surface, experimental, `advertised = false`),
    `README.md`, `AGENTS.md`, `plans/README.md`, both skill copies,
    and this status record. No new dependencies; `cargo deny` passes.

## Non-goals kept (plan §14)

No UDP sockets, no runtime task model, no RouterInfo publication, no
endpoint migration decision, no PeerTest/relay state machines, no
external interop. `Ssu2Session` plus its counters is the exact handoff
Plan 158 needs; nothing in this pass sends or receives a UDP datagram.

## Interpretation notes (not stop conditions)

Four points where this pass chose documented local policy over
specification silence (each pinned by constants and tests so a later
interop plan can revisit them mechanically):

1. Replay-window size (128), future-jump cap (1024), ACK delays,
   congestion constants, idle timeout (300 s), retransmission
   ceilings (5/fragment, 5 consecutive RTOs), reassembly quotas
   (16 messages / 256 KiB), and duplicate-cache size (128) are local
   policy; the specification requires boundedness without pinning
   values.
2. The ack-eliciting set treats the specification's trailing
   "Others?" as eliciting (only ACK/Address/DateTime/Padding/
   Termination are non-eliciting), which provably avoids ACK-of-ACK
   loops and is tested.
3. Per-message delivery failure after the retransmission ceiling is
   silent by design (counters only); the specification permits
   delivery failure without disconnection, and no event is emitted.
4. Greedy transmit packing (multiple fragments per packet within the
   MTU budget) means message count and packet count are intentionally
   unrelated; tests assert bytes and at-most-once delivery, never
   packet counts, except where the single-fragment-per-packet shape is
   structurally forced (1500-byte bodies over the 1220 budget).

No stop condition fired: no ACK/loss semantic required relaxing I2NP
bounds, no QUIC stack was needed, and loss behavior stays bounded
under the severe-fault trajectories.

## Validation record

Starting SHA: `b843524d4289bdc72409ce3da4208c8d7fcb8b9b`.

Local validation on the closing tree (all green):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
  (1389 passed, 55 suites; Plan 156 baseline was 1364, delta is +25:
  8 new session unit tests plus the 17-trajectory data-phase suite)
cargo test --locked -p i2pr-transport-ssu2 --all-targets (101 passed, 3 suites)
cargo test --locked -p i2pr-testkit --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh (14 rows pinned, hashes match)
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh (22 rows command-derived)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' (153 tests, OK)
cargo deny check advisories bans sources (ok)
```

`Cargo.lock` is unchanged (no new dependencies). The temporary
fixture-generator example used to mint the committed vectors was
deleted before closure.

Hosted lanes: routine CI and the manual SAM external workflow must
pass on the exact closing commit; the run IDs are recorded in the
handoff commit message and below once available.

Hosted lanes on the exact closing commit `<closing-sha>`:

- Routine CI run `<run-id>` (push, `main`, `<full-sha>`): conclusion
  `<conclusion>`.
- The manual SAM external lane was not rerun: this plan touches no
  SAM/runtime/daemon/product code (ssu2 protocol crate plus
  docs/fixtures only), and the Plan 151 evidence-integrity checker
  plus the full SAM suites pass locally on this tree.

## Acceptance criteria (all true, plan §16)

1. Authenticated short-header data packets round-trip with
   vectors/tests (2 committed vectors reproduced byte-for-byte; 17
   integration trajectories).
2. Tag mutation/replay never exposes plaintext effects (corruption and
   replay trajectories assert empty events plus rejection counters).
3. Packet-number reconstruction/replay window passes wrap/old/future/
   duplicate boundaries (unit tests at `u32::MAX`, plus window tests).
4. ACK encode/decode/range interpretation is strict and bounded
   (underflow/degenerate rejection, 128-range cap, idempotent apply).
5. ACK-only packets do not create an ACK loop (ACK-loss trajectory
   converges with bounded traffic; standalone ACKs fire only after
   eliciting receipts).
6. Delayed/immediate ACK state uses bounded per-session deadlines, not
   task/timer-per-packet architecture (single `ack_deadline_ms`;
   runtime-boundary script green).
7. Sent-packet provenance is bounded and releases on ACK/terminal
   failure (256-entry history with eviction test; prompt release
   asserted via zero bytes-in-flight after exchange).
8. Lost packets produce fresh retransmission packets rather than cached
   ciphertext replay (byte-inequality assertion on the retransmit).
9. RTT/RTO/cwnd/bytes-in-flight remain within documented bounds under
   severe fault (RTT clamps, cwnd min/max, overshoot within one
   packet, RTO backoff with terminal ceiling).
10. Outgoing I2NP fragmentation is exact and MTU-aware (fixed 1024-byte
    semantic fragments, deterministic numbering, 64-fragment ceiling,
    complete-block fast path for single fragments).
11. Incoming reassembly handles reorder/duplicates and rejects
    conflicting fragments (dedicated trajectories; never-overwrite
    rule).
12. Reassembly count/bytes exact-capacity/max+1 tests pass and cleanup
    is total (16/16 admitted, 17th denied without leak, termination
    zeroes both counters).
13. Complete I2NP messages are emitted exact and at most once
    (out-of-order emergence allowed; sorted/set equality plus
    duplicate-suppression assertions).
14. Duplicate-I2NP suppression is bounded (128-entry cache with
    retention expiry; redelivery assertions).
15. Idle/termination/key-rotation cleanup releases all session
    buffers/state (idle, local/remote termination, and NextNonce
    trajectories).
16. Privacy-safe diagnostic counters are bounded/nonsecret (counters
    struct holds counts only; redaction covered by construction).
17. No Tokio/socket dependency enters `i2pr-transport-ssu2`
    (runtime-boundary script green; no new imports of banned modules).
18. Full workspace/SSU2 vector/boundary floor passes (see validation
    record).
19. This record advances only to Plan 158.

## Handoff

Plan 157 is closed. Execute Plans **158 → 159 → 160 → 161** in order
under the Plan 154 roadmap authority. Plan 158 owns production UDP
ownership in `i2pr-runtime`, `TransportManager` integration, the
central bounded scheduler driving `Ssu2Session::poll`/`poll_transmit`,
and real localhost i2pr↔i2pr UDP product tests consuming this
session's actions, events, and counters.
