# `i2pr-testkit` — Deep Dive

Deterministic simulation crate for tests only. Provides manual clocks,
virtual stream/datagram links, scripted fault injection, reproducible
RNG, ephemeral peer factories, and an NTCP2 data-phase driver. Sits
outside the production dependency graph entirely.

Path: `crates/i2pr-testkit/`

`publish = false`, `#![forbid(unsafe_code)]`.

## Purpose

`i2pr-testkit` is the test-time seam that lets the rest of the
workspace run without wall-clock sleeps, real sockets, DNS, or
public-network traffic. It exercises the same crates through:

- A central `NetworkScheduler` that owns virtual stream and datagram
  link pairs and bounded delivery queues.
- A `ManualClock` that advances only when explicitly told to.
- A scripted `FaultScript` for drop / delay / duplicate / reorder /
  truncate / disconnect / reset with deterministic per-unit
  probability.
- A `ReproducibilitySeed` (128-bit, SHA-256 domain-separated
  derivation) and `DeterministicRng` (ChaCha8).
- A `PeerFactory` / `Topology` builder for synthesizing I2P peers.
- An `Ntcp2DataPhaseDriver` that drives `TransmitState` /
  `ReceiveState` one byte at a time.

## Module layout

| File | Lines | Responsibility | Key types |
| --- | --- | --- | --- |
| `src/lib.rs` | 422 | Crate root: `SimulationHarness`, `ReplayRecord`, re-exports, 7 inline unit tests | `SimulationHarness`, `ReplayRecord`, `MAX_SCENARIO_BYTES`, `HarnessError` |
| `src/clock.rs` | 417 | Manual and Tokio-backed monotonic clocks | `ManualClock`, `ManualInstant`, `MonotonicClock`, `MonotonicInstant`, `TokioClock`, `Deadline`, `ClockError`, `MAX_PENDING_TIMERS` |
| `src/faults.rs` | 426 | Deterministic fault scripts | `FaultAction`, `FaultError`, `FaultMatcher`, `FaultRule`, `FaultScript`, `FaultUnitKind`, `LinkDirection`, `LinkId`, `MAX_DUPLICATE_UNITS`, `MAX_FAULT_RULES` |
| `src/network.rs` | 1566 | The central scheduler; virtual stream and datagram pairs | `NetworkScheduler`, `SchedulerConfig`, `DatagramConfig`, `StreamConfig`, `StreamEndpoint`, `StreamError`, `DatagramEndpoint`, `DatagramPacket`, `DatagramError`, `SyntheticAddress`, `AdvanceReport`, `ReplayEvent`, `SchedulerSnapshot`, `MAX_DATAGRAM_SIZE`, `MAX_LINK_ID` |
| `src/ntcp2.rs` | 441 | Synchronous one-byte-at-a-time NTCP2 data-phase driver | `Ntcp2DataPhaseDriver`, `Ntcp2DriverCounters`, `Ntcp2DriverError`, `MAX_NTCP2_DRIVER_BUFFERED_BYTES` |
| `src/peers.rs` | 315 | Deterministic peer factory, `Topology` builder | `PeerFactory`, `PeerFactoryError`, `PeerId`, `PeerSummary`, `SyntheticServiceId`, `TestPeer`, `Topology`, `TopologyError`, `TopologyKind`, `MAX_TEST_PEERS` |
| `src/rng.rs` | 182 | `ReproducibilitySeed` and `DeterministicRng` | `ReproducibilitySeed`, `DeterministicRng`, `SeedDerivationError`, `SeedParseError`, `MAX_DOMAIN_LABEL_BYTES` |
| `src/transport.rs` | 86 | Synthetic helpers for `i2pr-transport` contract tests | `synthetic_transport_peer`, `synthetic_i2np_payload`, `synthetic_link_candidate`, `transport_manager_for_test`, `transport_resources_for_test`, `assert_payload_bounds`, `assert_snapshot_redaction`, `resource_usage` |

Integration tests:
- `tests/milestone_2.rs` (504 lines) — supervisor lifecycle,
  overload, restart-backoff, essential-failure teardown, fault
  replay, 32-seed soak matrix.
- `tests/milestone_3.rs` (186 lines) — transport contract
  validation, payload bounds, candidate/duplicate resolution,
  handshake/queue lease zeroing, snapshot redaction, 256-seed
  integrated matrix.
- `tests/ntcp2_handshake.rs` (79 lines) — handshake encode → one-byte
  delivery → decode round-trip.

## Public surface (highlights)

### `clock`
- `ManualInstant` (14), `MonotonicInstant` alias (34), `ClockError`
  (38), `Deadline` (63), `MonotonicClock` trait (78), `ManualClock`
  (131), `TokioClock` (353).

### `faults`
- `LinkId` (13), `LinkDirection` (33), `FaultUnitKind` (42),
  `FaultAction` (51), `FaultError` (101), `FaultMatcher` (130),
  `FaultRule` (255), `FaultScript` (284).

### `network`
- `MAX_LINK_ID`, `MAX_DATAGRAM_SIZE`, `SchedulerConfig`,
  `DatagramConfig`, `StreamConfig`, `SchedulerError`,
  `SyntheticAddress`, `NetworkScheduler` (649), `StreamEndpoint`
  (1284), `DatagramEndpoint` (1448), `StreamError`, `DatagramError`,
  `StreamLink`, `DatagramLink`, `DatagramPacket`,
  `SchedulerSnapshot`, `ReplayEvent`, `AdvanceReport`.

### `ntcp2`
- `Ntcp2DataPhaseDriver` (111), `Ntcp2DriverCounters` (22),
  `Ntcp2DriverError` (61), `MAX_NTCP2_DRIVER_BUFFERED_BYTES`.

### `peers`
- `PeerId` (15), `SyntheticServiceId` (25), `PeerSummary` (41),
  `TestPeer` (49), `PeerFactoryError` (93), `PeerFactory` (114),
  `TopologyKind` (186), `TopologyError` (199), `Topology` (217),
  `MAX_TEST_PEERS`.

### `rng`
- `MAX_DOMAIN_LABEL_BYTES`, `ReproducibilitySeed` (13),
  `SeedDerivationError` (99), `SeedParseError` (121),
  `DeterministicRng` (146).

### Root items (`lib.rs`)
- `SimulationHarness` (58), `MAX_SCENARIO_BYTES` (68), `HarnessError`
  (71), `ReplayRecord` (240).

## Key abstractions

### `ManualClock` (`clock.rs:131`)
Clonable `Arc`-backed monotonic clock; advances only when explicitly
told to. Holds a `BTreeMap<(nanos, sequence), Arc<Waiter>>` of
pending sleepers. `advance(duration)` bumps the internal
nanosecond counter, collects all entries whose deadline ≤ new time,
and wakes them in order. The `MonotonicClock` trait provides a
narrow async interface (`now`, `deadline_after`, `sleep_until`).
`TokioClock` is a thin wrapper around `tokio::time::Instant` for
integration tests that need real Tokio time.

### `NetworkScheduler` (`network.rs:649`)
Central deterministic pump. **No background task** — callers
manually invoke `advance()` or `advance_to_next_event()`. Internally:
a `BTreeMap<DeliveryKey, Delivery>` ordered by `(deadline, link,
direction, order_sequence, sequence, duplicate_index)`. On advance,
it pops due deliveries and pushes payloads into target `StreamState`
or `DatagramState` queues. A `ResourceBudget` tracks pending timers,
buffered bytes, and link leases. `stream_link` / `datagram_link`
create symmetric pairs of `StreamEndpoint` / `DatagramEndpoint`
connected through the scheduler. Endpoints expose `try_write` /
`try_read` (non-blocking, synchronous) and `write_until` /
`read_until` / `send_until` / `recv_until` (async, using
`ManualClock` sleep + `tokio::select!` + `CancellationToken`).

### Virtual sockets
- `StreamEndpoint` (`network.rs:1284`) — byte-stream with bounded
  receive capacity, segment-limited writes, graceful half-close
  (`shutdown()`), hard reset (`reset()`), and backpressure via
  `tokio::sync::Notify`.
- `DatagramEndpoint` (`network.rs:1448`) — message-oriented with
  bounded packet count and byte capacity. Each `DatagramPacket`
  carries a `SyntheticAddress` source. Complete datagram boundaries
  are preserved.
- **Reservation pattern**: endpoints call `reserve()` before
  scheduling and `release()` on drop/reset, preventing
  double-counting and ensuring resource lease return to zero.

### Fault injection (`faults.rs`)
`FaultScript` holds a bounded `Vec<FaultRule>` (max 64). Each rule
has a `FaultMatcher` and a `FaultAction`. On schedule, `apply()`
evaluates rules in declaration order:

`Drop` clears units; `Delay` adds deterministic nanoseconds;
`Duplicate` expands units (max 8 extra); `Reorder` swaps
`order_sequence` within a window; `Truncate` shortens payloads;
`Disconnect` drains then EOF; `Reset` discards and resets.
Probability is deterministic per-unit via SHA-256 domain-separated
derivation from the seed.

### `SimulationHarness` (`lib.rs:58`)
Thin synchronous orchestrator binding `ManualClock`,
`NetworkScheduler`, `CancellationToken`, and `ReproducibilitySeed`.
Provides `run_until(predicate, max)`, `advance_to_next_event()`,
`advance(duration)`, `run_until_idle(max)`, `replay()`, `shutdown()`.
**Never spawns tasks** — it is a manual pump.

### `ReproducibilitySeed` (`rng.rs:13`)
128-bit seed (`[u8; 16]`) with SHA-256 domain-separated derivation
(`b"i2pr-testkit/domain-separation/v1\0" || seed || len(label) ||
label`, truncated to 128 bits). `Display`/`FromStr` for
32-hex-char round-trip. `DeterministicRng` wraps `ChaCha8Rng`
seeded from the 128-bit seed doubled to 32 bytes.

### `Ntcp2DataPhaseDriver` (`ntcp2.rs:111`)
Synchronous, bounded, one-byte-at-a-time driver connecting
`TransmitState` → `ReceiveState`. `queue_plaintext` seals a frame;
`write_one` transfers one byte from outbound to inbound;
`read_one` consumes one byte and returns a completed frame when
the length prefix + ciphertext are fully received.
`pump_until_idle(max)` runs the full cycle. Tracks
`Ntcp2DriverCounters`. No sockets, no Tokio, no async.

### `PeerFactory` / `Topology` (`peers.rs`)
`PeerFactory` derives deterministic Ed25519/X25519 key material
from `ReproducibilitySeed::derive("identity/{index}")` via
`DeterministicRng`. `TestPeer` wraps a `RouterIdentityBundle` with
non-`Debug`, non-cloneable private material. `Topology::build`
creates edge lists for `Linear` / `Star` / `Ring` / `Arbitrary`
shapes, and `stream_links` / `datagram_links` instantiate them
through a `NetworkScheduler`.

## Dependencies

| Dependency | Source | Purpose |
| --- | --- | --- |
| `i2pr-core` | workspace path | Resource budgets, service contracts |
| `i2pr-crypto` | workspace path | Router identity bundles |
| `i2pr-proto` | workspace path | `RouterInfo`, `Date`, `Hash`, `Mapping` |
| `i2pr-runtime` | workspace path | `CancellationToken`, supervisor, service graph |
| `i2pr-transport` | workspace path | Transport contracts, `TransportManager` |
| `i2pr-transport-ntcp2` | workspace path | Frame owners, handshake codecs |
| `rand_chacha` | workspace | `ChaCha8Rng` |
| `rand_core` | workspace | `RngCore`, `SeedableRng` |
| `sha2` | workspace | SHA-256 for seed derivation |
| `tokio` | workspace | `sync::Notify`, `select!`, `test` |

`tokio` is permitted: `i2pr-testkit` is one of only two crates
(alongside `i2pr-runtime`) allowed to depend on Tokio. Compliant.

**Zero reverse dependency on production crates.** No production
`Cargo.toml` references this crate. `publish = false` reinforces
this.

## Tests

Inline unit tests:
- `src/lib.rs:256-422` — 7 unit tests on seed derivation, manual
  clock, stream ordering, datagram boundaries, fault replay,
  peer factory, harness idle/shutdown.
- `src/ntcp2.rs:360-441` — 4 unit tests on one-byte pump round-trip,
  multi-frame buffering, disconnect cleanup, buffer-limit rejection.

Integration tests under `tests/`:
- `milestone_2.rs` (7 tests).
- `milestone_3.rs` (5 tests).
- `ntcp2_handshake.rs` (1 test).

How to run:
```bash
cargo test -p i2pr-testkit --all-targets
```

All async tests use `#[tokio::test(start_paused = true)]`.

## Distinctive design choices

1. **Zero wall-clock, zero network.** Every timeout is
   `ManualClock::advance()`. Every I/O is in-memory.
2. **Fault probability is deterministic per-unit.** SHA-256
   derivation from `(seed, rule_id, link, direction, kind,
   sequence)` → `u32 % 1_000_000`. Same seed + same sequence =
   same outcome, always.
3. **Replay records are privacy-safe.** `ReplayRecord` contains no
   payload bytes or secret material — only `ReplayEvent` metadata.
4. **One-byte-at-a-time NTCP2 driving.** Exercises frame
   length-prefix decoding and ciphertext reassembly edge cases that
   bulk transfers would mask.
5. **Resource leases return to zero.** Every `StreamEndpoint` /
   `DatagramEndpoint` reserves capacity and releases on drop,
   reset, or delivery. The 256-seed soak asserts zero resource
   usage at teardown.
6. **`SimulationHarness` never spawns.** Callers spawn and join
   explicitly within the harness scope; `shutdown()` cancels via
   `CancellationToken` and closes the scheduler.
7. **Domain-separated seed derivation.** The constant domain string
   prevents cross-crate seed collisions.
8. **`#![forbid(unsafe_code)]`** — no unsafe blocks.

## Cross-references

- [Overview](overview.md)
- [i2pr-runtime](i2pr-runtime.md) — exercised through the harness.
- [i2pr-transport](i2pr-transport.md) — `src/transport.rs` provides
  synthetic helpers for contract tests.
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — driver on top
  of `TransmitState`/`ReceiveState`.
- Plan-of-record: `plans/023-m2-deterministic-network-testkit.md`;
  closure: `plans/023-closure.md`.
