# Aggregate Milestone 2 closure: supervised runtime, bounded communication, simulation, and observability

- Status: Complete for the bounded, non-networked scope
- Date: 2026-07-15
- Plans: 021, 022, 023, and 024

## Scope and non-claims

Milestone 2 now has concrete supervision, wakeable hierarchical cancellation,
bounded channels and resource leases, deterministic manual simulation, and a
privacy-aware observation boundary. Plan 024 integrates these pieces with
fixed-seed service-graph and link-fault scenarios.

No milestone code opens a socket, performs DNS, exchanges peer traffic,
touches NetDB as network behavior, builds a tunnel, exposes a client/API or
administrative listener, advertises a capability, or changes protocol support
statuses. The milestone proves local ownership, cleanup, bounded overload, and
replay behavior only. It is not anonymity, privacy, resilience,
authentication, interoperability, or production-readiness evidence.

## Implementation and closure history

The four plan implementation/closure points are:

| Scope | Commit | Evidence |
| --- | --- | --- |
| Plan 021 supervision and cancellation implementation | `3c8137b` | `plans/021-closure.md` |
| Plan 021 CI evidence record | `727d254` | normal/MSRV evidence status recorded there |
| Plan 022 bounded channels and resource governance implementation | `9b099da` | `plans/022-closure.md` |
| Plan 022 CI evidence record | `531fab4` | normal/MSRV evidence status recorded there |
| Plan 023 deterministic network testkit implementation and closure | `1a32ab7` | `plans/023-closure.md` |
| Remote CI evidence availability record | `69dffbc` | remote evidence was unavailable at that point |
| Plan 024 implementation, validation, and aggregate closure | this commit | local matrix below; remote run link to be added after push |

## Final dependency graph

```text
i2pr-proto <- i2pr-crypto <- i2pr-storage
      ^
i2pr-core <- i2pr-runtime <- i2pr-daemon
      ^             ^
      +------- i2pr-testkit (test/simulation only)
```

`i2pr-runtime` is the only production crate that owns Tokio tasks, timers,
channels, or wakeable cancellation. `i2pr-testkit` may depend on runtime
contracts for tests, but no production crate depends on it. Plan 024 adds the
already-reviewed workspace `tracing` dependency to `i2pr-runtime`; it adds no
new external package or runtime/network feature.

## Runtime ownership and lifecycle

| Classification | Failure policy | Shutdown owner | Validation |
| --- | --- | --- | --- |
| Essential | cancel the graph and return typed failure | supervisor manager | clean and essential-failure scenarios |
| Restartable | bounded exponential backoff; explicit degrade/shutdown exhaustion | service manager under supervisor | one-recovery path and existing exhaustion tests |
| Degradable | remain visible as degraded; dependents are marked degraded | supervisor and service scope | existing runtime tests and clean graph |
| Optional | record failure without changing router readiness | supervisor and service scope | clean and forced-shutdown scenarios |

Graph validation completes before any task is spawned. Startup is deterministic
dependency-first; readiness is one-shot; health is latest-state. Every manager
is in the supervisor `JoinSet`. Every service child is in a bounded child
scope. Shutdown cancels all scopes, joins to the configured deadline, aborts
remaining managers, joins aborted handles, and reports graceful versus forced
cleanup. The new task counters decrement on normal joins and forced child-scope
cleanup; final snapshots must show zero owned service and child tasks.

## Communication and resource contracts

Command/request queues wait only with a caller deadline and cancellation scope;
event queues are bounded drop-newest; latest-state channels coalesce and expose
versions. Queue admission occurs before resource admission, and accepted queue
items carry their lease until receiver handoff or drop. The infrastructure
capacity ceiling remains 4,096 slots and caller byte estimates remain capped at
1 MiB.

Resource budgets are immutable and immediate grant-or-deny. Bundles validate and
commit atomically; leases are non-cloneable and release on drop or consuming
release. Snapshot entries report limit, usage, high-water, and saturating
denials. The overload scenarios exercise capacities 1, 2, and 4, exact load,
over-limit load, typed full/resource-denial results, receiver-drop cleanup,
and zero final usage.

## Clock, seed, links, and replay

Runtime deadline tests use paused Tokio time and explicit `time::advance`.
Testkit scenarios use `ManualClock`, fixed root seeds, stable scenario names,
and bounded pumps. `ReproducibilitySeed::derive` keeps component streams
domain-separated. Stream links preserve ordered partial bytes and half-close;
datagram links preserve complete packet boundaries and synthetic sources.

Fault rules compose in declaration order for bounded delay, duplication,
reordering, truncation, drop, disconnect, and reset actions. Replay records
retain only root seed, scenario, link/direction/kind/sequence/rule/outcome
metadata, monotonic time, bounded steps, and aggregate resource/queue state.
They contain no payloads, keys, destinations, RouterInfo values, or real
addresses.

## Observability and privacy review

`i2pr-runtime::event` defines fixed names for service registration, startup,
readiness, degradation, failure, restart, stopping/stopped, shutdown,
channel rejection, resource denial, simulation fault, and simulation
completion. Event fields are limited to validated static identifiers, typed
categories, bounded counters with units, monotonic timing, and synthetic
simulation metadata. Lower crates never install a subscriber; the daemon owns
subscriber configuration.

`SupervisorSnapshot` omits health detail text and exposes service lifecycle,
readiness, classification, restart count, typed failure, transition sequence,
and monotonic transition time. `RuntimeSnapshot::try_new` sorts and caps
channel/resource observations and accepts aggregate simulation counters. The
snapshot is eventually coherent across independent owners and does no await
while holding mutable runtime state. `HealthDetail` has a bounded internal
value but a redacted default `Debug` implementation. Default events/snapshots
do not expose keys, session material, reply tags, payloads, identities,
destinations, full hashes, addresses, filesystem paths, panic/error text,
precise per-peer timing histories, or dynamic peer labels.

## Integrated scenario inventory

`crates/i2pr-testkit/tests/milestone_2.rs` provides:

1. Clean startup/shutdown with essential, restartable, degradable, and
   optional services, bounded command channel, zero final tasks/queues/resources.
2. Bounded overload with typed resource denial, capacity assertions, lease
   release on receiver drop, and responsive graceful shutdown.
3. Restart recovery with one deterministic failure, one-second backoff, and a
   ready replacement whose restart count is visible.
4. Essential failure after readiness, privacy-safe typed completion, dependent
   cancellation, and forced abort of a deliberately non-cooperative optional
   service with zero remaining managers.
5. Stream/datagram fault execution and same-seed replay for delay, duplication,
   reordering, truncation, drop, reset, and disconnect.

The fixed soak matrix compares two complete replays for root seeds `0..31`.
Boundary coverage also includes capacities 1, 2, and 4; zero resource limits
rejected as illegal; exact and over-limit demand; restart recovery and existing
restart exhaustion coverage; graceful and forced shutdown; and bounded timer,
pending-delivery, byte, stream, and datagram behavior from Plan 023.

## Cleanup invariants

| Owner | Creation bound | Normal cleanup | Forced/cancel cleanup | Final assertion |
| --- | --- | --- | --- | --- |
| Supervisor manager | graph service maximum | `JoinSet::join_next` | abort all then join | zero service tasks |
| Service child scope | 64 children | child scope join | abort on scope drop, count released | zero child tasks |
| Channel queue item | channel capacity | receiver handoff | queued-item drop | zero queue depth/lease usage |
| Manual-clock sleeper | `MAX_PENDING_TIMERS` | wake at deadline | clock close/cancel | zero pending timers |
| Scheduled link delivery | scheduler pending/byte limits | delivery handoff | purge on close/reset | zero deliveries/bytes |
| Synthetic endpoint/link | configured link budget | endpoint/link drop | scheduler close plus owner drop | zero active links |
| Replay/test capture | bounded event vector | record comparison | test scope drop | no payload/private material |

## Exact local validation results

All commands ran from the repository root on 2026-07-15 with the pinned
Rust 1.95.0 toolchain unless a command names another toolchain:

```text
rtk cargo fmt --all --check                              PASS
rtk cargo check --workspace                              PASS
rtk cargo check --workspace --all-targets                PASS
rtk cargo test --workspace                               PASS: 126 tests
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
                                                         PASS
RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps
                                                         PASS
rtk bash scripts/check-dependency-direction.sh            PASS
rtk bash scripts/check-runtime-boundaries.sh              PASS
rtk cargo deny check advisories bans sources              PASS
rtk cargo +1.85.0 check --workspace --all-targets         PASS
rtk cargo test -p i2pr-testkit --all-targets              PASS: 15 tests
rtk git diff --check                                     PASS
```

`cargo deny` reports the pre-existing duplicate `rand_core` major versions
required by `x25519-dalek` and the workspace `rand_core`; advisories, bans, and
sources pass. No fixture bytes changed, so the fixture manifest check and fuzz
smoke lane were not required. No model-checking dependency was added: the
bounded deterministic tests exercise the same production accounting and
cancellation code without a second implementation.

## CI evidence

The repository CI definition still covers pinned Ubuntu/macOS quality,
Rust 1.85 MSRV, dependency policy, documentation warnings, and dependency
direction. Prior availability status is recorded by `69dffbc`. This closure
commit is being pushed to `main` to produce fresh runs; the exact GitHub Actions
run URL must be appended here after the remote run is created. A remote run
that is unavailable or incomplete must be recorded explicitly rather than
treated as local evidence.

## Deviations, dependency/security decisions, and limitations

- No transport, protocol, NetDB, tunnel, client, listener, plugin, exporter,
  persistent event history, or capability API was added.
- Aggregate snapshots are bounded and redacted but eventually coherent across
  independently owned channels, resources, and simulation state.
- `HealthDetail` remains an internal bounded value for typed service context;
  its public default `Debug` output is now redacted and aggregate snapshots
  omit it entirely.
- Observability uses the existing workspace `tracing` dependency only. No
  subscriber is installed below the daemon and no high-cardinality labels are
  introduced.
- The deterministic matrix is fixed and local; it is not a public-network,
  mixed-router, kernel-buffer, MTU, NAT, transport-authentication, or anonymity
  test.

## Milestone 3 prerequisites

Before NTCP2 planning or implementation, demonstrate that a transport can be
represented as an essential/restartable supervised service; reader/writer
children remain in service scopes; handshake/link queues use existing bounded
channels and leases; transport timeouts use the clock contract; replay and
test handshakes use domain-separated seeds; in-memory links model partial I/O,
delay, truncation, disconnect, and backpressure; and transport-category
tracing uses no peer-derived labels. Do not add sockets or handshake code as
part of this gate.
