# Plan 157 — Milestone 8 SSU2 v2 data-phase reliability and fragmentation

Status: **registered; execute after Plan 156 passes**.

Depends on Plan 156. Blocks Plan 158.

## 1. Goal

Implement the runtime-neutral authenticated SSU2 v2 data phase: short-header protection, packet-number/replay handling, ACK ranges and scheduling, bounded loss/retransmission/congestion state, I2NP fragmentation/reassembly, duplicate-message suppression, rekey/termination/idle behavior, and deterministic severe-fault tests.

No UDP sockets are opened in this pass.

## 2. Session-state ownership

Build one explicit `Ssu2Session` or equivalent owner from Plan 156's authenticated output.

It should own, with hard ceilings:

```text
directional data keys/header-protection state
send packet-number state
receive packet-number replay window
pending ACK state/ranges
RTT/RTO/loss state
congestion-window state
sent-packet -> I2NP-fragment provenance
pending retransmission fragments
reassembly state
I2NP duplicate-suppression state
idle/key-rotation/termination state
```

All time/randomness comes from caller-supplied inputs/actions. No Tokio or wall-clock calls.

Avoid retaining complete ciphertext/datagrams solely for retransmission. Retain the smallest semantic fragment/provenance state needed to construct a fresh packet.

## 3. Short header and authenticated packet processing

Implement exact v2 short-header/header-protection behavior using the established keys.

Receive order must be:

1. cheap structural/datagram-size check;
2. identify candidate session by connection ID using caller context;
3. unprotect/decode header and reconstruct packet number;
4. replay/window admissibility check;
5. authenticate/decrypt payload;
6. parse bounded blocks;
7. only then expose I2NP/control effects.

A tag failure or replay must not mutate application-visible receive state.

Transmit path must:

- allocate a new packet number within bounds;
- assemble current ACK/control/I2NP-fragment blocks;
- AEAD seal once;
- apply header protection;
- return one bounded datagram action plus semantic sent-packet accounting.

## 4. Packet-number reconstruction and replay window

Implement the current SSU2 packet-number truncation/reconstruction algorithm exactly.

Required properties:

- bounded sliding receive window;
- reject already-seen packet numbers;
- reject excessively old packets;
- reject impossible future jumps according to the chosen/spec policy;
- handle truncated-number wrap boundaries correctly;
- replay decision occurs before blocks are delivered;
- window storage is fixed-size or otherwise strictly bounded, not an ever-growing set.

Tests must cover boundary/wrap/reorder/duplicate cases around the replay-window edges.

## 5. ACK block semantics and scheduling

Implement ACK block interpretation using the exact current v2 encoding from Plan 155.

Requirements:

- `ackThrough`/range representation validated for underflow/overflow;
- maximum ACK/NACK range count bounded;
- malformed overlapping/impossible ranges rejected;
- received ACKs retire only actually-sent packet state;
- duplicate ACK information is idempotent;
- ACK-only packets must not trigger ACK-of-ACK loops;
- ACK-only packets follow the spec's congestion-accounting exception;
- delayed ACK scheduling is represented by one per-session deadline/state, not a timer/task per packet;
- immediate ACK conditions use the current SSU2 guidance and hard lower/upper timer bounds;
- piggyback ACKs and standalone ACK actions share one coherent acknowledgement view.

Add deterministic tests proving an ACK-only exchange converges without infinite traffic.

## 6. Sent-packet provenance and retransmission

Track for each congestion-controlled sent packet only the bounded metadata required for loss recovery:

```text
packet number
sent time
ack-eliciting bytes
semantic I2NP fragment references / retransmittable control items
retransmission generation/count where needed
```

When a packet is declared lost:

- do not replay the original encrypted datagram byte-for-byte;
- mark still-needed I2NP fragments/control items for fresh packet assembly;
- the new packet gets a new packet number and current ACK state;
- already-delivered/acknowledged semantic fragments are not retransmitted unnecessarily;
- retransmission attempts have explicit ceilings/terminal policy.

ACK bookkeeping must release sent-packet/resource accounting promptly.

## 7. RTT/RTO and conservative congestion controller

Where v2 permits implementation choice, use a simple auditable controller rather than importing a QUIC stack.

Requirements:

- byte-count congestion window with explicit min/default/max;
- bytes-in-flight tracked exactly for congestion-controlled packets;
- RTT sample only from eligible acknowledged packets;
- smoothed RTT/variance/RTO with explicit lower/upper clamps;
- timeout/loss reduces window conservatively;
- successful acknowledgements permit bounded growth;
- no signed/unsigned overflow;
- severe loss cannot create unbounded retransmission metadata;
- ACK-only packets excluded from bytes-in-flight where the spec requires;
- one central `poll(now)`/action mechanism drives timers.

Document algorithm choices where SSU2 leaves room for implementation policy. Correctness/resource predictability takes priority over throughput.

## 8. I2NP fragmentation

Implement outgoing fragmentation for encoded I2NP messages that exceed one packet's available payload.

Requirements:

- consume existing bounded `EncodedI2npMessage` or the appropriate shared encoded-I2NP type;
- exact first/follow-on fragment headers;
- deterministic fragment numbering;
- conservative payload sizing based on negotiated/current MTU and block overhead;
- hard maximum fragments per message;
- no zero-length ambiguous fragments;
- each fragment retains enough semantic identity for fresh retransmission;
- sender releases fragment backing storage after all required fragments are acknowledged or terminal failure occurs.

Do not add a stream abstraction. SSU2 transports messages.

## 9. I2NP reassembly

Implement strict per-session/global reassembly budgets.

Each in-progress message should be keyed by the protocol's message/fragment identity and retain:

- expected/known fragment metadata;
- received-fragment bitmap/ranges;
- exact total bytes retained;
- creation/last-progress deadline;
- conflict detection state.

Rules:

- fragments may arrive out of order;
- duplicates are idempotent and do not duplicate retained bytes;
- conflicting duplicate fragment data terminates/drops according to protocol policy, never silently overwrites;
- aggregate bytes/messages/fragments have exact caps;
- max+1 admissions fail closed without partial state leak;
- expiration releases all buffers;
- complete message is emitted once and removed atomically;
- message length must remain within existing I2NP maximums.

Tests must intentionally saturate one session without starving an unrelated session if the resource model supports scoped quotas.

## 10. Duplicate I2NP suppression

Packet replay protection is not sufficient because retransmitted fragments may produce semantically duplicate I2NP delivery under some trajectories.

Add a bounded recently-delivered message-ID/expiry cache at the SSU2 session/delivery boundary as required by the spec.

- bounded count/retention;
- expiration based on message lifetime/current protocol rules;
- duplicate I2NP never delivered twice to the transport consumer;
- cache pressure fails safely without becoming an unbounded map.

Do not silently suppress distinct messages that reuse an ID outside the permitted retention context; match I2NP semantics.

## 11. Data-phase control, key rotation, termination, idle

Implement v2 control needed before runtime integration:

- NewToken block handoff/update policy;
- DateTime/Options handling required in data phase;
- termination reason handling;
- idle timeout actions;
- key/session rotation/rekey behavior required by current v2 spec;
- path challenge/response block plumbing as typed events, while actual endpoint migration policy is Plan 159;
- relay/peer-test blocks decoded into typed events but their state machines remain Plan 160.

A Termination block or local terminal condition must release pending retransmit/reassembly state under bounded cleanup.

## 12. Deterministic network/fault driver

Add test-only state-machine driving support using `i2pr-testkit` from tests, without creating a production dependency.

Required scenarios:

- no loss, bidirectional multi-message exchange;
- one DATA packet lost -> fresh retransmission -> exact message once;
- one ACK packet lost -> recovery without ACK loop;
- duplicate packet;
- severe reorder beyond several packets;
- authenticated corruption -> no block/I2NP delivery;
- replay old packet;
- ACK-range boundary/malformed range;
- fragmentation with first/middle/final fragment loss;
- fragment reorder/duplicate/conflicting duplicate;
- reassembly exact capacity/max+1;
- sent-history exact capacity/max+1;
- prolonged heavy loss reaches bounded retry/termination policy;
- idle timeout;
- key-rotation boundary if v2 uses one;
- two independent sessions under fault, proving one session's pressure does not corrupt the other.

The local semantic acceptance should explicitly allow complete I2NP messages to emerge out of order where SSU2 permits; exact per-message bytes and at-most-once delivery remain required.

## 13. Observability

Expose only small privacy-safe counters/snapshots needed for Plan 158/161 evidence:

```text
packets_sent/received
packets_replayed/rejected
acks_sent/received
loss_events
retransmitted_fragments
bytes_in_flight
cwnd
reassembly_messages/bytes
reassembly_drops
termination category
```

No payload bytes, token values, raw keys, or unbounded endpoint histories.

## 14. Non-goals

No:

- UDP sockets;
- runtime task model;
- RouterInfo publication;
- endpoint migration decision;
- PeerTest/relay state machines;
- external interop.

## 15. Validation

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-testkit --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
```

## 16. Acceptance criteria

Plan 157 passes only when:

1. authenticated short-header data packets round-trip with vectors/tests.
2. tag mutation/replay never exposes plaintext effects.
3. packet-number reconstruction/replay window passes wrap/old/future/duplicate boundaries.
4. ACK encode/decode/range interpretation is strict and bounded.
5. ACK-only packets do not create an ACK loop.
6. delayed/immediate ACK state uses bounded per-session deadlines, not task/timer-per-packet architecture.
7. sent-packet provenance is bounded and releases on ACK/terminal failure.
8. lost packets produce fresh retransmission packets rather than cached ciphertext replay.
9. RTT/RTO/cwnd/bytes-in-flight remain within documented bounds under severe fault.
10. outgoing I2NP fragmentation is exact and MTU-aware.
11. incoming reassembly handles reorder/duplicates and rejects conflicting fragments.
12. reassembly count/bytes exact-capacity/max+1 tests pass and cleanup is total.
13. complete I2NP messages are emitted exact and at most once.
14. duplicate-I2NP suppression is bounded.
15. idle/termination/key-rotation cleanup releases all session buffers/state.
16. privacy-safe diagnostic counters are bounded/nonsecret.
17. no Tokio/socket dependency enters `i2pr-transport-ssu2`.
18. full workspace/SSU2 vector/boundary floor passes.
19. `plans/157-status.md` advances only to Plan 158.

## 17. Stop conditions

Stop and narrow if:

- spec/reference behavior disagrees on ACK/loss semantics that materially changes delivery;
- correct loss recovery appears to require a large third-party QUIC implementation;
- reassembly requires relaxing existing I2NP size bounds;
- test evidence shows the chosen congestion policy can grow memory/state proportional to offered loss.

Do not compensate by increasing ceilings until tests pass.