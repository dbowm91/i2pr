# `i2pr-testkit` — Deep Dive

Deterministic, bounded simulation primitives for local state-machine
tests only. Provides manual clocks, virtual stream/datagram links,
scripted fault injection, a reproducible seed/RNG, ephemeral peer
factories, a synchronous NTCP2 data-phase driver, and a
payload-free Streaming fingerprint schema.

Path: `crates/i2pr-testkit/`

`publish = false`, `#![forbid(unsafe_code)]`
(`crates/i2pr-testkit/src/lib.rs:11`). Eight private modules, no
public module is re-exported — only the items in the
`pub use` lists below are reachable.

## Purpose

`i2pr-testkit` is a **test-only** crate. It is the seam that lets
supervised services, transport contracts, and the NTCP2 frame owners
be exercised without wall-clock sleeps, real sockets, DNS, or
public-network traffic. It provides:

- A central `NetworkScheduler` owning virtual stream and datagram
  link pairs plus bounded delivery queues.
- A `ManualClock` whose virtual time advances only when a caller says
  so.
- A scripted `FaultScript` with drop / delay / duplicate / reorder /
  truncate / disconnect / reset and deterministic per-unit
  probability.
- A `ReproducibilitySeed` (128-bit, SHA-256 domain-separated
  derivation) and `DeterministicRng` (ChaCha8).
- A `PeerFactory` / `Topology` builder for synthetic I2P peers.
- An `Ntcp2DataPhaseDriver` driving `TransmitState` / `ReceiveState`
  one byte at a time, synchronously.
- A payload-free `StreamingFingerprintTrace` schema for cross-family
  Streaming observations.

It must **not** own: sockets, DNS, timers, background tasks, network
reachability, or any production behavior. The crate doc comment
(`src/lib.rs:3-6`) states the boundary verbatim:

> `i2pr-testkit` is intentionally outside the production dependency
> graph. It opens no sockets, performs no DNS lookups, and never
> contacts the I2P network. Simulation is a controllable model of
> queues and failures, not a transport-interoperability claim.

That last sentence is load-bearing: a passing testkit scenario is
evidence about *bounded local state machines*, never about wire
compatibility with another I2P implementation. Interop evidence comes
only from the `#[ignore]`-gated external lanes and their
`scripts/check-*-evidence.sh` checkers.

`src/transport.rs:1-5` repeats the restriction locally: those
factories "create local identifiers and bounded bytes only. They do
not create router identities, sockets, addresses, or interoperability
fixtures."

## Module layout

Line counts from `wc -l crates/i2pr-testkit/src/*.rs`.

| File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- |
| `src/lib.rs` | 429 | Crate root: module wiring, re-exports, `SimulationHarness`, `ReplayRecord`, 8 inline unit tests | `SimulationHarness`, `HarnessError`, `ReplayRecord`, `MAX_SCENARIO_BYTES` |
| `src/clock.rs` | 417 | Manually advanced and Tokio-backed monotonic clocks | `ManualClock`, `ManualSleep`, `ManualInstant`, `MonotonicInstant`, `MonotonicClock`, `Deadline`, `TokioClock`, `TokioSleep`, `ClockError`, `MAX_PENDING_TIMERS` |
| `src/faults.rs` | 428 | Bounded deterministic fault scripts and their matcher/action vocabulary | `FaultScript`, `FaultRule`, `FaultMatcher`, `FaultAction`, `FaultError`, `LinkId`, `LinkDirection`, `FaultUnitKind`, `MAX_FAULT_RULES`, `MAX_DUPLICATE_UNITS` |
| `src/network.rs` | 1565 | The central manual scheduler; virtual stream and datagram pairs | `NetworkScheduler`, `SchedulerConfig`, `StreamConfig`, `DatagramConfig`, `StreamLink`, `StreamEndpoint`, `DatagramLink`, `DatagramEndpoint`, `DatagramPacket`, `SyntheticAddress`, `AdvanceReport`, `SchedulerSnapshot`, `ReplayEvent`, `SchedulerError`, `StreamError`, `DatagramError`, `MAX_LINK_ID`, `MAX_DATAGRAM_SIZE` |
| `src/ntcp2.rs` | 441 | Synchronous one-byte-at-a-time NTCP2 data-phase driver | `Ntcp2DataPhaseDriver`, `Ntcp2DriverCounters`, `Ntcp2DriverError`, `MAX_NTCP2_DRIVER_BUFFERED_BYTES` |
| `src/peers.rs` | 315 | Deterministic ephemeral peer factory and topology builder | `PeerFactory`, `TestPeer`, `PeerSummary`, `PeerId`, `SyntheticServiceId`, `Topology`, `TopologyKind`, `PeerFactoryError`, `TopologyError`, `MAX_TEST_PEERS` |
| `src/rng.rs` | 198 | `ReproducibilitySeed` and `DeterministicRng` | `ReproducibilitySeed`, `DeterministicRng`, `SeedDerivationError`, `SeedParseError`, `MAX_DOMAIN_LABEL_BYTES` |
| `src/streaming_fingerprint.rs` | 470 | Bounded, identity-free Streaming fingerprint schema and cross-family classification | `StreamingFingerprintTrace`, `FingerprintEvent`, `FingerprintScenario`, `FingerprintDirection`, `FingerprintTerminal`, `FingerprintClassification`, `FingerprintTraceError`, `classify_dimension`, `normalize_sequence`, `MAX_FINGERPRINT_EVENTS`, `FINGERPRINT_TIME_BUCKET_MS`, `FINGERPRINT_DEADLINE_MS` |
| `src/transport.rs` | 86 | Synthetic helpers for the `i2pr-transport` contract tests | `synthetic_transport_peer`, `synthetic_i2np_payload`, `synthetic_link_candidate`, `transport_manager_for_test`, `transport_resources_for_test`, `assert_payload_bounds`, `assert_snapshot_redaction`, `resource_usage` |

Total ≈ 4.3k lines of source.

Integration tests (`wc -l crates/i2pr-testkit/tests/*.rs`):

- `tests/milestone_2.rs` (504 lines, 7 tests) — supervisor lifecycle,
  bounded overload, restart backoff, essential-failure teardown,
  fault replay, 32-seed soak matrix, explicit capacity boundaries.
- `tests/milestone_3.rs` (186 lines, 5 tests) — transport contract
  validation, payload bounds, candidate/duplicate resolution,
  queue/handshake lease zeroing, snapshot redaction, 256-seed
  integrated matrix.
- `tests/ntcp2_handshake.rs` (79 lines, 1 test) — handshake encode →
  one-byte delivery → decode round-trip over a virtual stream.

## Public surface

`lib.rs:13-56` declares eight private modules and re-exports exactly
the following.

### `clock` (`src/lib.rs:22-25`)
`ClockError`, `Deadline`, `MAX_PENDING_TIMERS`, `ManualClock`,
`ManualInstant`, `MonotonicClock`, `MonotonicInstant`, `TokioClock`.
`ManualSleep` and `TokioSleep` are `pub` in `clock.rs` but are the
associated `Sleep` types of the trait and are not re-exported.

### `faults` (`src/lib.rs:26-29`)
`FaultAction`, `FaultError`, `FaultMatcher`, `FaultRule`, `FaultScript`,
`FaultUnitKind`, `LinkDirection`, `LinkId`, `MAX_DUPLICATE_UNITS`,
`MAX_FAULT_RULES`. `FaultTerminal`, `PlannedFaultUnit`, and
`FaultPlan` are `pub(crate)`.

### `network` (`src/lib.rs:30-34`)
`AdvanceReport`, `DatagramConfig`, `DatagramEndpoint`, `DatagramError`,
`DatagramLink`, `DatagramPacket`, `MAX_DATAGRAM_SIZE`, `MAX_LINK_ID`,
`NetworkScheduler`, `ReplayEvent`, `SchedulerConfig`, `SchedulerError`,
`SchedulerSnapshot`, `StreamConfig`, `StreamEndpoint`, `StreamError`,
`StreamLink`, `SyntheticAddress`. `ReplayOutcome`
(`network.rs:1174`) is `pub` in the module but is **not** re-exported,
so it is not nameable through the crate root even though
`ReplayEvent::outcome` is a public field.

### `ntcp2` (`src/lib.rs:35-37`)
`MAX_NTCP2_DRIVER_BUFFERED_BYTES`, `Ntcp2DataPhaseDriver`,
`Ntcp2DriverCounters`, `Ntcp2DriverError`.

### `peers` (`src/lib.rs:38-41`)
`MAX_TEST_PEERS`, `PeerFactory`, `PeerFactoryError`, `PeerId`,
`PeerSummary`, `SyntheticServiceId`, `TestPeer`, `Topology`,
`TopologyError`, `TopologyKind`.

### `rng` (`src/lib.rs:42-45`)
`DeterministicRng`, `MAX_DOMAIN_LABEL_BYTES`, `ReproducibilitySeed`,
`SeedDerivationError`, `SeedParseError`.

### `streaming_fingerprint` (`src/lib.rs:46-51`)
`FINGERPRINT_DEADLINE_MS`, `FINGERPRINT_TIME_BUCKET_MS`,
`FingerprintClassification`, `FingerprintDirection`, `FingerprintEvent`,
`FingerprintScenario`, `FingerprintTerminal`, `FingerprintTraceError`,
`MAX_FINGERPRINT_EVENTS`, `StreamingFingerprintTrace`,
`classify_dimension`, `normalize_sequence`.

### `transport` (`src/lib.rs:52-56`)
`assert_payload_bounds`, `assert_snapshot_redaction`, `resource_usage`,
`synthetic_i2np_payload`, `synthetic_link_candidate`,
`synthetic_transport_peer`, `transport_manager_for_test`,
`transport_resources_for_test`.

### Root items defined in `lib.rs`
`SimulationHarness` (`lib.rs:65`), `MAX_SCENARIO_BYTES = 64`
(`lib.rs:75`), `HarnessError` (`lib.rs:78`), `ReplayRecord`
(`lib.rs:247`).

## Key contracts

### `ManualClock` (`clock.rs:131`)
`Arc`-backed, clonable, monotonic; **time only moves on
`advance()`**. The state (`clock.rs:114-127`) is a `Mutex<ClockState>`
holding `now: u64`, `next_sequence: u64`, and
`pending: BTreeMap<(u64, u64), Arc<Waiter>>` — ordered by
`(deadline_nanos, registration_sequence)`.

- `with_max_timers(maximum)` rejects `0` or any value above
  `MAX_PENDING_TIMERS = 4096` (`clock.rs:10`, `176-181`).
- `advance(duration)` checked-adds the duration, collects every key
  with `deadline <= new now`, and wakes those sleepers in order
  (`clock.rs:206-232`).
- `sleep_until(deadline)` returns a `ManualSleep`; its `poll` registers
  a `Waiter`, stores the waker, and yields `ClockError::TimerLimit`
  rather than growing past the bound. Dropping a `ManualSleep`
  unregisters its key (`clock.rs:341-348`).
- `Drop for ManualClock` marks the clock closed and wakes every pending
  waiter, so no sleeper leaks past the last handle
  (`clock.rs:150-167`).

`ClockError` variants (`clock.rs:38-45`): `Overflow`, `Closed`,
`TimerLimit { maximum }`.

`MonotonicClock` (`clock.rs:78`) is the narrow contract both clocks
implement: `now()`, `deadline_after(Duration)`, and
`sleep_until(Deadline)` with an associated `Sleep: Future<Output =
Result<(), ClockError>>`. `Deadline` (`clock.rs:63`) is a clock-relative
instant with `is_expired(now)` and `instant()`.
`MonotonicInstant` is an alias for `ManualInstant` (`clock.rs:34`).
`TokioClock` (`clock.rs:353`) wraps `tokio::time::Instant` with a
recorded origin for the rare test that needs real Tokio time.

This pair is the backbone of the repo's "no wall-clock sleeps in
state-machine tests" rule: every deadline a test observes is a
`ManualClock` deadline the test itself advanced.

### `NetworkScheduler` (`network.rs:650`)
A clonable, deterministic pump. **There is no scheduler task.** Callers
invoke `advance(duration)` / `advance_to_next_event()` /
`has_pending()` / `snapshot()` / `close()` / `replay(seed, scenario,
steps)` themselves.

Internals: pending work lives in
`BTreeMap<DeliveryKey, Delivery>` ordered by `(deadline, link,
direction, order_sequence, sequence, duplicate_index)`
(`network.rs:272-279`), so ordering is total and reproducible.
`next_sequence` is tracked per `(LinkId, LinkDirection)`. Each scheduled
unit first acquires a bundled `ResourceBudget` lease for
`PendingTimers` + `BufferedBytes` (`network.rs:870-872`).

`NetworkScheduler::new` builds a `ResourceBudget` from the config plus
the `SimulatedStreamLinks` / `SimulatedDatagramLinks` classes, each
capped at `MAX_LINKS = 1_024` (`network.rs:656-670`). `with_budget`
accepts a Plan 022 budget instead.

Link abstractions:

- `stream_link(link, StreamConfig, FaultScript) -> StreamLink` —
  `StreamLink::left()` / `right()` hand out two `StreamEndpoint`s
  (`network.rs:706-740`). `StreamConfig::new(receive_capacity,
  max_segment_bytes)` rejects zero, and `max_segment_bytes >
  receive_capacity`; the default is 64 KiB receive / 1 KiB segment.
  `StreamEndpoint` exposes synchronous `try_write` / `try_read`
  (non-blocking, segment-limited) plus async `write_until(bytes,
  deadline, cancellation)` and `read_until(buf, deadline,
  cancellation)`, which combine `tokio::sync::Notify`,
  `ManualClock::sleep_until`, and `CancellationToken` under
  `tokio::select!` (`network.rs:1343-1390`). `shutdown()` half-closes
  after queued bytes drain; `reset()` closes the peer target and
  discards queued bytes.
- `datagram_link(link, DatagramConfig, FaultScript) -> DatagramLink` —
  `network.rs:743-780`. `DatagramConfig::new(max_datagram_size,
  receive_capacity)`; defaults are 1200 bytes / 64 packets, and
  `max_datagram_size` may not exceed `MAX_DATAGRAM_SIZE = 65_535`
  (`network.rs:20`). `DatagramEndpoint` offers `try_send` / `try_recv`
  and async `send_until` / `recv_until`; each `DatagramPacket` carries a
  `SyntheticAddress` source and complete message boundaries are
  preserved.

Bounding and lease hygiene: every queue and byte charge goes through
`reserve()` / `release_reservation()` on the endpoint state
(`network.rs:321`, `462`) and the `Target`-level reservation
(`network.rs:564-570`). `SchedulerSnapshot` reports
`pending_deliveries`, `buffered_bytes`, `pending_timers`,
`stream_links`, `datagram_links`, `closed`, and `resource_usage`, and
carries no payload bytes.

### `FaultScript` (`faults.rs:286`)
Holds a `seed` plus at most `MAX_FAULT_RULES = 64` `FaultRule`s;
`FaultScript::new` rejects an over-long rule list and any
`Duplicate` whose `copies` is `0` or above
`MAX_DUPLICATE_UNITS = 8` (`faults.rs:293-305`).
`FaultRule::new(id: u16, matcher, action)` gives each rule a stable
diagnostic id.

`FaultMatcher` (`faults.rs:130`) is a builder of optional constraints —
`link`, `direction`, `kind`, `sequence`, `sequence_range(start, end)`,
`every(n)` (matches every n-th unit from sequence zero), and
`probability_ppm(ppm)` (0..=1_000_000). Unset fields match anything.

`apply()` (`faults.rs:325-405`) walks the rules in declaration order,
stops at the first terminal action, and produces a plan of planned
units:

| `FaultAction` | Effect |
| --- | --- |
| `Drop` | Clear all units; terminal `Drop`; stop. |
| `Delay(d)` | Checked-add `d` to every unit's delay; `DelayOverflow` on overflow. |
| `Duplicate { copies }` | Append `copies` tagged copies; `ExpansionLimit` if the expanded set exceeds `MAX_DUPLICATE_UNITS + 1`. |
| `Reorder { window }` | Rewrite `order_sequence` to reverse order inside each `window`-sized group. |
| `Truncate { max_bytes }` | Truncate every unit's payload to `max_bytes`. |
| `Disconnect` | Terminal `Disconnect` (drain then EOF). |
| `Reset` | Clear units; terminal `Reset`. |

Probability is per-unit and deterministic: a 16-byte label of
`link(u32 BE) ‖ direction ‖ kind ‖ rule_id(u16 BE) ‖ sequence(u64 BE)`
is fed through `ReproducibilitySeed::derive_bytes`, and the leading
four bytes as big-endian `u32` modulo 1_000_000 are compared against
`probability_ppm` (`faults.rs:233-251`).

### `ReproducibilitySeed` / `DeterministicRng` (`rng.rs`)
`ReproducibilitySeed([u8; 16])` (`rng.rs:13`) is built with
`from_bytes`, `from_u128` (big-endian), or `as_bytes`. `derive(label)`
rejects an empty label or one longer than
`MAX_DOMAIN_LABEL_BYTES = 64` (`rng.rs:9`, `32-42`) and hashes
`b"i2pr-testkit/domain-separation/v1\0" ‖ seed ‖ len(label) as u16 BE ‖
label` with SHA-256, truncating to 128 bits (`rng.rs:44-54`). The
`Display`/`FromStr` pair round-trips through exactly 32 lowercase hex
characters (an optional `0x` prefix is accepted on parse), so a seed
is safe to print in a failure message and paste back into a rerun.

`DeterministicRng` (`rng.rs:146`) wraps `ChaCha8Rng` seeded from the
16-byte seed **doubled** into 32 bytes (`rng.rs:153-161`); `child(label)`
derives a new domain seed so sub-generators never share mutable state.
It implements `RngCore` and `TryCryptoRng`.

### `Ntcp2DataPhaseDriver` (`ntcp2.rs:111`)
Synchronous and bounded — no sockets, no clock, no async. It holds one
`TransmitState`, one `ReceiveState`, and three `VecDeque`/`Vec` byte
buffers. `new(transmit, receive, maximum_buffered_bytes)` requires
`FRAME_OVERHEAD <= maximum_buffered_bytes <=
MAX_NTCP2_DRIVER_BUFFERED_BYTES = 1 << 20` (`ntcp2.rs:14`, `124-142`).

`queue_plaintext` seals a frame and returns its wire length;
`write_one` moves at most one byte from the pending writer to the
stream; `read_one` consumes at most one byte, decodes the 2-byte length
prefix once available, and returns a `ReceivedFrame` when the full
ciphertext has arrived; `pump_one` pairs the two; `pump_until_idle(max)`
runs the bounded cycle and returns the completed frame count.
`disconnect()` releases every retained byte and reports
`TruncatedFrame` if a partial frame was in flight — cleanup happens
before the error is returned, so counters stay readable.
`Ntcp2DriverCounters` tracks queued/written/read/received frames and
bytes plus `buffered_bytes`, `peak_buffered_bytes`, `released_bytes`,
`discarded_bytes`, and `disconnected`.
`Ntcp2DriverError` variants: `InvalidBufferLimit`,
`BufferLimit { buffered, requested, maximum }`, `Disconnected`,
`TruncatedFrame`, `Frame(FrameError)`, `StepLimit { maximum }`.

### `PeerFactory` / `Topology` (`peers.rs`)
`PeerFactory::new(seed, maximum)` / `bounded(seed)` cap peers at
`MAX_TEST_PEERS = 128` (`peers.rs:11`, `121-133`). `peer(index)`
derives `ReproducibilitySeed::derive("identity/{index}")`, fills signing
and encryption key bytes from a `DeterministicRng`, and builds a
`RouterIdentityBundle` (`peers.rs:135-161`). `TestPeer` holds the
private bundle with a hand-written `Debug` that prints only `TestPeer`
and its id — never key material — and `router_info(published_millis)`
signs an in-memory, **no-capability** `RouterInfo` (`peers.rs:48-89`).
`Topology::build` accepts `Linear`, `Star`, `Ring`, or explicit
`Arbitrary(Vec<(usize, usize)>)` edges, sorts and de-duplicates them
(rejecting `InvalidEdge` / `DuplicateEdge`), and
`stream_links` / `datagram_links` instantiate the edges through a
`NetworkScheduler` with stable `LinkId`s.

### `Transport virtual links and helpers (`src/transport.rs`)
Eight free functions rather than types: `synthetic_transport_peer`,
`synthetic_i2np_payload`, `synthetic_link_candidate` (already advanced
through handshake and authentication), `transport_resources_for_test`,
`transport_manager_for_test` (both on `TransportLimits::for_test()`),
`assert_payload_bounds` (asserts zero, maximum, and maximum+1),
`assert_snapshot_redaction` (asserts the snapshot `Debug` contains
neither `PeerId` nor `EncodedI2npMessage`), and `resource_usage`.

### `StreamingFingerprintTrace` (`src/streaming_fingerprint.rs`)
Purpose: a **schema**, not a transport. It defines the bounded,
identity-free observation format used to compare Streaming behavior
across i2pr, Java, and i2pd, so a dimension can be classified without
capturing raw bytes. The module header is explicit: the types
"intentionally carry no Destination hash, peer address, packet bytes,
or application payload" (`streaming_fingerprint.rs:3-4`).

- `MAX_FINGERPRINT_EVENTS = 4096`,
  `FINGERPRINT_TIME_BUCKET_MS = 10`, `FINGERPRINT_DEADLINE_MS =
  600_000` (10 minutes); an event past the last bucket is rejected.
- `FingerprintScenario` fixes an 11-value vocabulary
  (`clean_handshake` … `abrupt_close`) shared by all three
  implementations; `FingerprintDirection` is `to_destination` /
  `from_destination`; `FingerprintTerminal` is `established`,
  `orderly_close`, `reset`, `retransmit_limit`, `deadline`,
  `transport_failure` (peer/error text is never retained).
- `FingerprintEvent` holds `index`, `time_bucket_10ms`, `direction`,
  `flags`, `payload_len`, `sequence_delta`, `acknowledgement_delta`,
  `retransmission_ordinal`, `max_payload`, `advertised_window`,
  `choked`, `terminal`. `normalize_sequence(origin, value)` strips the
  random sequence origin while preserving wrapping deltas.
- `StreamingFingerprintTrace::push` enforces event count, contiguous
  `index`, monotonic time buckets, and the scenario deadline.
  `to_tsv()` / `from_tsv()` round-trip a fixed 13-column header;
  `from_tsv` accepts only the canonical form it emits.
- `FingerprintTraceError`: `BadHeader`, `BadRow`, `EmptyTrace`,
  `MixedScenarios`, `EventLimit`, `IndexOrder`, `TimeOrder`,
  `DeadlineExceeded`.
- `classify_dimension(i2pr, java, i2pd) -> FingerprintClassification`
  turns three optional readings into `CommonAcrossAll`,
  `I2prMatchesJava`, `I2prMatchesI2pd`,
  `ReferencesDifferI2prMatchesNeither`, `NotReliablyObservable`, or
  `HarnessLimitation` — so a harness defect is never reported as a
  behavioral difference.

Evidence path: `scripts/check-streaming-fingerprint-evidence.sh
<plan312-evidence-dir>` validates the manifest (`i2pd_version 2.61.0`,
exact revision pin, scenario `clean_handshake_default_port`, dimensions
`flags,from_included,max_payload,payload_length`, `raw_packet_bytes 0`,
`destination_or_stream_ids 0`), requires all four role traces
(`i2pr-client`, `i2pd-client`, `i2pr-server`, `i2pd-server`) with the
canonical header and exactly one SYN/SYN-ACK row each, and writes
`fingerprint-matrix.tsv` with 8 compared dimensions. That checker
verifies the *shape* of already-collected interop observations; the
simulation in this crate is not what fills it in.

## Dependencies

`crates/i2pr-testkit/Cargo.toml`:

| Dependency | Source | Purpose |
| --- | --- | --- |
| `i2pr-core` | workspace path | `ResourceBudget` / `ResourceClass` / lease types, `CancellationReason` |
| `i2pr-crypto` | workspace path | `RouterIdentityBundle` for synthetic peers |
| `i2pr-proto` | workspace path | `RouterInfo`, `Date`, `Hash`, `Mapping` |
| `i2pr-runtime` | workspace path | `CancellationToken`, `ServiceGraph` / supervisor, used by `milestone_2.rs` |
| `i2pr-transport` | workspace path | `TransportManager`, `TransportLimits`, `EncodedI2npMessage`, `LinkCandidate` for `transport.rs` |
| `i2pr-transport-ntcp2` | workspace path | `TransmitState` / `ReceiveState` / handshake codecs for the driver and `ntcp2_handshake.rs` |
| `rand_chacha` | workspace | `ChaCha8Rng` |
| `rand_core` | workspace | `RngCore`, `SeedableRng`, `TryCryptoRng` |
| `sha2` | workspace | SHA-256 seed and probability derivation |
| `tokio` | workspace | `sync::Notify`, `select!`, `time::Instant`, `#[tokio::test]` |

These six workspace edges match the allowlist entry for
`"i2pr-testkit"` in `scripts/check-dependency-direction.sh:24-27`
exactly, and the checker fails on any additional *normal* `i2pr-*`
dependency.

### Why `tokio` is allowed here
`scripts/check-runtime-boundaries.sh:29-36` scans every
`crates/*/Cargo.toml` and rejects a `tokio` or `tokio-util` entry
unless the crate is `i2pr-runtime` or `i2pr-testkit`. Testkit is on
that allowlist precisely because it depends on `i2pr-runtime` and must
drive the same Tokio primitives the runtime owns: `tokio::sync::Notify`
for endpoint backpressure, `tokio::select!` for deadline/cancellation
races, `tokio::time::Instant` behind `TokioClock`, and
`#[tokio::test(start_paused = true)]` for supervised-service
scenarios. Testkit is not claiming runtime neutrality — it is a
consumer of the one crate that owns Tokio.

### No production crate may depend on the testkit
Enforced in `scripts/check-runtime-boundaries.sh:38-42`:

```bash
testkit_dependents=$(grep -En 'i2pr-testkit' "$root/crates"/*/Cargo.toml || true)
if printf '%s\n' "$testkit_dependents" | grep -Ev 'crates/i2pr-testkit/Cargo.toml' | grep -Eq .; then
  echo "production crate depends on i2pr-testkit" >&2
  exit 1
fi
```

Every `crates/*/Cargo.toml` is matched, so the ban covers
`[dev-dependencies]` too — not just `[dependencies]`. Verified: the
only hit in the workspace is
`crates/i2pr-testkit/Cargo.toml:2` (its own `name` field), plus the
`members` list in the root `Cargo.toml:17`. No other crate declares
testkit as a normal or dev dependency.

`crates/i2pr-daemon/tests/streaming_tunnel_external.rs:50-51` shows
the sanctioned alternative for cross-crate reuse: it includes the
schema with
`#[path = "../../i2pr-testkit/src/streaming_fingerprint.rs"]` and a
comment stating the intent — "reuses the testkit schema without adding
a package dependency from the daemon crate to i2pr-testkit." Note the
practical consequence: source-included copies must be kept in sync by
hand; the boundary checker does not and cannot verify that.

The same script adds testkit-specific discipline: no unbounded
channels, no `std::thread::sleep` / `mem::forget`, owned
`tokio::spawn` only, and no raw `JoinHandle` in
`crates/i2pr-testkit/**` (`check-runtime-boundaries.sh:6-27`).

## Tests

```bash
cargo test --locked -p i2pr-testkit --all-targets -- --test-threads=1
```

Verified locally: **29 tests, all passing** — 16 in-crate unit tests,
7 in `milestone_2.rs`, 5 in `milestone_3.rs`, 1 in `ntcp2_handshake.rs`.

In-crate `#[cfg(test)]` modules:

- `src/lib.rs:262-429` — 8 tests: seed-domain independence and
  hex round-trip; equal-deadline sleeper wake order; ordered partial
  stream delivery; datagram boundary and source preservation;
  executable + replay-safe faults; reproducible file-less peer
  factory; a fixed seed matrix replaying identically; harness
  idle/teardown reaching zero pending deliveries, bytes, and timers.
- `src/ntcp2.rs:360-441` — 4 tests: one-byte pump round-trip with
  buffers released to zero; multiple frames on one stream with bounded
  peak; disconnect cleaning a partial frame and exposing
  `discarded_bytes`; rejection of a frame over the byte bound.
- `src/streaming_fingerprint.rs:370-470` — 4 tests: canonical,
  payload-free TSV round-trip; wrapping-preserving sequence
  normalization; index/time/deadline/mixed-scenario rejections;
  non-collapsing classification across the three families.

### Determinism guarantees and replaying a failure

1. Time only moves through `ManualClock::advance`; the two async
   harnesses use `#[tokio::test(start_paused = true)]`
   (`lib.rs:280`, `milestone_2.rs:65,120,170,238`) and
   `tokio::task::yield_now`, never a wall-clock sleep.
2. Every bounded loop takes a caller-supplied cap —
   `run_until(maximum, predicate)`, `run_until_idle(maximum)`,
   `pump_until_idle(maximum_steps)`, `with_max_timers` — and returns a
   typed `StepLimit { maximum }` rather than spinning.
3. Ordering is total: `(deadline, link, direction, order_sequence,
   sequence, duplicate_index)` plus per-`(link, direction)` sequence
   counters. Same seed and same operations give byte-identical
   delivery order.
4. Fault probability is a pure function of
   `(seed, rule_id, link, direction, kind, sequence)`.

To replay a failure, read the seed from the `ReplayRecord` (or the
seed's `Debug`/`Display`, which is 32 hex characters) and pin the
scenario name — `ReplayRecord` carries `seed`, `scenario`, `events`,
`final_time`, `snapshot`, and `steps` and is `Eq`-comparable, which is
exactly what `scenario_simulated_link_faults_replay_identically`,
`scenario_fixed_32_seed_soak_matrix_is_reproducible` (seeds `0..32`),
`fixed_seed_matrix_replays_identically`, and
`fixed_seed_integrated_matrix_covers_256_bounded_schedules`
(seeds `0..=255`) assert by running the same scenario twice and
comparing records. `ReplayRecord` and `ReplayEvent` hold metadata
only — no payload bytes and no secret material — so a failure record
is safe to print.

## Distinctive design choices

1. **No scheduler task.** `NetworkScheduler` is a pure manual pump;
   tests own every pump step, so ordering is never a race.
2. **Virtual time is caller-driven.** Every timeout in a testkit
   scenario is a `ManualClock::advance`, which is why the crate has no
   `std::thread::sleep` and the boundary checker can forbid one.
3. **Total delivery ordering.** The `DeliveryKey` tuple makes
   tie-breaking explicit, so equal-deadline units never reorder between
   runs.
4. **Deterministic per-unit fault probability.** SHA-256 over a
   fixed-width `(link, direction, kind, rule_id, sequence)` label —
   no RNG state and no wall clock in the decision.
5. **Domain-separated seed derivation.** The
   `i2pr-testkit/domain-separation/v1\0` prefix plus a length-prefixed
   label makes cross-crate seed collisions impossible, and the
   32-hex `Display`/`FromStr` pair makes seeds copy-pasteable.
6. **Secrets never reach `Debug`.** `TestPeer` prints only its id;
   `assert_snapshot_redaction` and `SchedulerSnapshot` keep payloads
   and peer identities out of diagnostics.
7. **Fingerprint schema, not a capture.** The Streaming fingerprint
   types hold counts, flags, and deltas only; `classify_dimension`
   separates `HarnessLimitation` from a real behavioral difference.
8. **One-byte-at-a-time NTCP2 driving.** The driver and
   `ntcp2_handshake.rs` exercise length-prefix decoding and ciphertext
   reassembly that bulk transfers would mask, and assert buffers
   return to zero on every path including disconnect.
9. **Leases return to zero.** `ScenarioSnapshot`-style teardown
   assertions require `pending_deliveries`, `buffered_bytes`,
   `pending_timers`, and the link classes back to zero, and `ResourceBudget`
   charges are released on every drop path.
10. **`#![forbid(unsafe_code)]`** plus `publish = false`: not
    publishable, and not a place where unsafe can accumulate.

## Cross-references

- [Overview](overview.md)
- [Dependency graph](dependency-graph.md) — mirrors
  `scripts/check-dependency-direction.sh`; the testkit allowlist and
  the "no production crate may depend on `i2pr-testkit`" rule.
- [Tooling](tooling.md) — the boundary, evidence, and lane scripts.
- [i2pr-core.md](i2pr-core.md) — `ResourceBudget` / `ResourceClass`
  (`PendingTimers`, `BufferedBytes`, `SimulatedStreamLinks`,
  `SimulatedDatagramLinks`) and `CancellationReason`.
- [i2pr-runtime.md](i2pr-runtime.md) — `CancellationToken`, supervisor,
  and `ServiceGraph`, exercised end to end by `tests/milestone_2.rs`.
- [i2pr-transport.md](i2pr-transport.md) — `TransportManager`,
  `TransportLimits`, `EncodedI2npMessage`, and `LinkCandidate`, driven
  by `src/transport.rs`.
- [i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md) —
  `Ntcp2DataPhaseDriver` on top of `TransmitState` / `ReceiveState`,
  plus the handshake codecs used by `tests/ntcp2_handshake.rs`.
- [i2pr-crypto.md](i2pr-crypto.md) — `RouterIdentityBundle` behind
  `PeerFactory`.
- [interop-apparatus.md](interop-apparatus.md) — the separate,
  evidence-producing interop lane; testkit simulation is not interop
  evidence.
- [ADR 0001: Modular monolith](../adr/0001-modular-monolith.md) —
  `i2pr-testkit` as a workspace member alongside core and daemon.
- [ADR 0002: Tokio runtime boundary](../adr/0002-tokio-runtime-boundary.md) —
  why the testkit may control protocol time and keep Tokio.
- [ADR 0013: NTCP2 data phase and blocks](../adr/0013-ntcp2-data-phase-and-blocks.md) —
  names `crates/i2pr-testkit/src/ntcp2.rs` as the frame-boundary driver.
- [ADR 0030: Destination linkability domains, service lifecycle, and i2pd Streaming](../adr/0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md)
  — authority for the Plan 312 fingerprint baseline.
- Plan of record:
  `plans/implementation/workspace-foundation/023-m2-deterministic-network-testkit.md`;
  closure: `plans/closure/workspace-foundation/023-closure.md`.
  Related: Plan 022 (bounded channels and resource governance,
  `plans/closure/workspace-foundation/022-closure.md`) and Plan 036
  (transport/adversarial validation, named in
  `tests/milestone_3.rs:1`,
  `plans/closure/ntcp2-transport/036-closure.md`). Fingerprint baseline:
  `plans/implementation/anonymity/312-i2pd-streaming-directional-fingerprint-baseline.md`,
  closed in `plans/closure/anonymity/312-status.md`.
  See `plans/README.md` for the registry.
