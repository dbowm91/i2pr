# Plan 023 closure: deterministic network testkit

- Status: Complete for the bounded, non-networked simulation scope
- Date: 2026-07-15
- Plan: [`023-m2-deterministic-network-testkit.md`](023-m2-deterministic-network-testkit.md)

## Scope and deviations

Plan 023 turns `i2pr-testkit` into a deterministic manual-pump simulation
boundary. It does not open sockets, resolve names, contact the public I2P
network, persist identity material, add production fault-injection hooks, or
claim transport interoperability.

The harness intentionally does not spawn a scheduler task or own an implicit
service graph. It exposes the Plan 021 runtime cancellation token and accepts
Plan 022 resource budgets; any Tokio service used by a simulation remains
owned by the caller's supervisor or child scope. This keeps task ownership
auditable and avoids introducing a second supervision abstraction.

Reorder faults use deterministic reversal of bounded sequence groups rather
than a separately running reorder task. Stream and datagram APIs remain
testkit-specific; Tokio `AsyncRead`/`AsyncWrite` adapters are deferred until a
transport plan proves their exact accounting and fault semantics.

## Changed files

- `crates/i2pr-testkit/Cargo.toml`: add test-only workspace dependencies on
  core, crypto, proto, runtime, SHA-256, and Tokio.
- `crates/i2pr-testkit/src/lib.rs`: public façade, manual harness, replay
  record, deterministic seed and teardown tests.
- `crates/i2pr-testkit/src/clock.rs`: bounded manual sleepers, checked
  deadlines, teardown wakeups, and Tokio clock adapter.
- `crates/i2pr-testkit/src/rng.rs`: domain-separated SHA-256 seed derivation
  and deterministic ChaCha8 streams.
- `crates/i2pr-testkit/src/faults.rs`: bounded matchers, scripts, composition,
  duplicate limits, and explicit disconnect/reset semantics.
- `crates/i2pr-testkit/src/network.rs`: bounded deterministic scheduler,
  stream/datagram links, backpressure, cancellation/deadline operations,
  resource accounting, snapshots, and replay events.
- `crates/i2pr-testkit/src/peers.rs`: ephemeral deterministic identities,
  no-capability RouterInfo values, peer summaries, and topology factories.
- `scripts/check-dependency-direction.sh`: permit only the planned testkit
  dependencies; production crates remain independent of testkit.
- `README.md`, `AGENTS.md`, `CONTRIBUTING.md`: current scope, seed/replay
  reproduction, privacy, and no-public-network rules.
- `docs/architecture.md`: simulation ownership, ordering, bounds, and link
  semantics.
- `docs/security-model.md`: non-equivalence, fault-injection, replay privacy,
  and teardown threats.
- `plans/023-closure.md`: this closure record.

The protocol support ledger is unchanged. No protocol dossier or support claim
was broadened.

## Public contracts and limits

- `ManualClock` stores at most 4,096 pending sleepers by default and wakes due
  waiters in deadline/registration order. There is no wall-clock fallback.
- `ReproducibilitySeed` is 128 bits, parses/display-round-trips as 32 hex
  characters, and derives independent domains from labels capped at 64 bytes.
- The scheduler caps pending deliveries at 4,096 and queued bytes at 1 MiB.
  Its ordering key is `(deadline, link, direction, order-sequence, sequence,
  duplicate-index)`.
- Stream receiver queues and segment sizes are explicit; stream reads are
  ordered bytes with partial reads and graceful EOF/reset. Datagram queues
  preserve complete boundaries and use synthetic addresses.
- Fault scripts cap rules at 64 and duplicate expansion at eight extra units.
  Matching is metadata-only and probability decisions derive from the root
  seed and unit metadata.
- Peer factories cap a factory at 128 peers. Private identity seeds are
  generated in memory and held by the existing zeroizing crypto wrappers.
- Replay records contain only seed, scenario, rule IDs, sequence numbers,
  outcomes, monotonic time, and bounded snapshots. Payloads, private keys,
  destinations, real addresses, and full RouterInfo values are absent.
- Link leases are held by live endpoint handles and release on handle drop.
  Scheduler shutdown purges pending deliveries and resets endpoint waiters;
  tests must drop endpoint handles before asserting zero active links.

## Test evidence

The focused testkit suite covers:

- seed formatting, parse round trips, and domain separation;
- equal-deadline manual sleepers and timer cleanup;
- ordered partial stream delivery;
- datagram boundary and synthetic-source preservation;
- duplicate fault execution and replay-safe rule metadata;
- deterministic private-peer factory behavior and no-capability RouterInfo;
- harness idle pumping and queued-work teardown.

The ordinary deterministic seed matrix is zero, all ones, and fixed regression
seeds `1`, `2`, `3`, `4`, and `5`. No OS-random seed or wall-clock sleep is
used by the testkit tests.

## Quality results

Results are recorded here after the final local pass:

```text
cargo fmt --all --check                                      PASS
cargo check --workspace --all-targets                         PASS
cargo test --workspace                                       PASS (117 tests)
cargo test -p i2pr-testkit --all-targets                      PASS (8 tests)
cargo clippy --workspace --all-targets --all-features -- -D warnings PASS
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps  PASS
bash scripts/check-dependency-direction.sh                    PASS
cargo deny check advisories bans sources                      PASS (existing rand_core duplicate warning)
cargo +1.85.0 check --workspace --all-targets                PASS
git diff --check                                              PASS
```

CI evidence is recorded after the direct main-branch push. The workflow is
configured for both Ubuntu and macOS quality jobs, Ubuntu MSRV, and dependency
policy; a missing or unavailable remote run is recorded explicitly rather than
inferred as a pass.

## Dependency and security decisions

The testkit is the only crate allowed to consume the new simulation dependencies;
the production dependency direction remains unchanged. Tokio is used only by
the non-production testkit API and its tests. No real network feature,
filesystem identity path, payload-bearing diagnostic, or production fault
hook was added. The manual scheduler owns no task, and every accepted pending
unit owns its Plan 022 resource lease until delivery, purge, or drop.

## Known limitations and Plan 024 handoff

- The scheduler models bounded queueing and deterministic faults, not kernel
  buffers, MTU, checksums, NAT, IPv4/IPv6, transport authentication, or I2P
  wire interoperability.
- The manual harness does not automatically construct a supervisor graph;
  callers must register any long-lived simulation services with Plan 021.
- Reorder groups are deterministic and bounded but are not a general network
  jitter model.
- Endpoint leases intentionally outlive scheduler shutdown until endpoint
  handles are dropped, making ownership visible instead of forcibly invalidating
  live owners.
- No mixed-router, public-network, anonymity, privacy, or production-readiness
  evidence follows from this milestone.

Plan 024 may consume the testkit for authorized transport state-machine tests.
It must preserve the clock, seed, ordering, capacity, replay redaction, and
task-ownership contracts documented here.
