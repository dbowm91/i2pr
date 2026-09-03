# Plan 158 — Milestone 8 SSU2 UDP runtime and local session product

Status: **registered; execute after Plan 157 passes**.

Depends on Plan 157. Blocks Plan 159.

## 1. Goal

Bind the runtime-neutral SSU2 v2 implementation to real UDP sockets owned by `i2pr-runtime`, integrate the existing generic `TransportManager`, and prove a complete i2pr↔i2pr authenticated SSU2 local product over real localhost datagrams.

This is the first M8 pass that opens sockets.

## 2. Architecture constraint

All production socket/timer/task ownership remains in `i2pr-runtime`.

The protocol crate must remain unchanged in character:

```text
i2pr-transport-ssu2
  pure bounded protocol/state machines

        actions/events
             ↕

i2pr-runtime
  UdpSocket + time + OS RNG + ChildScope + bounded queues

             ↕

i2pr-transport
  generic link/resource/delivery manager
```

Do not add Tokio to `i2pr-transport-ssu2`.

## 3. Configuration

Add an explicit `[ssu2]` daemon/runtime configuration surface with strict unknown-field handling.

Initial safe defaults:

```text
enabled = false
bind_ipv4 = loopback / disabled according to current config conventions
bind_ipv6 = disabled unless explicitly enabled
port = 0 allowed for tests; validated normal range for configured service
advertise = false
introducer_service = false
```

Resource/deadline settings should have conservative defaults and hard validation ceilings:

- max pending handshakes;
- max active sessions;
- per-IP/subnet pending/session limits;
- datagram queue capacity/bytes;
- token table limits;
- reassembly/global bytes;
- handshake timeout/retry ceiling;
- idle timeout;
- scheduler poll bounds.

Do not expose dozens of protocol-tuning knobs merely because internal constants exist.

## 4. UDP socket ownership

Implement a runtime owner for UDP sockets, preferably one owner per address family:

```text
Ssu2RuntimeService
  IPv4 socket owner
  IPv6 socket owner (structurally supported, optional activation)
  bounded session registry
  bounded pending-handshake registry
  central timer/loss scheduler
  transport-manager handle
```

Rules:

- `UdpSocket` construction/bind only in runtime;
- separate IPv4/IPv6 socket state where endpoint/family semantics require it;
- set/validate receive/send datagram ceilings at the application layer regardless of kernel buffer size;
- one receive task per active socket is acceptable;
- no task per packet;
- no task per ACK/retransmission timer;
- all spawned tasks owned by `ChildScope` and shut down within bounded deadlines.

## 5. Cheap receive classification

The UDP receive loop must do the cheapest safe checks before expensive work or state allocation.

Suggested ordering:

1. receive into bounded fixed/max-sized buffer;
2. reject impossible datagram length;
3. classify known SSU2 version/type/header family;
4. reject wrong network ID where visible;
5. look up active connection ID or pending handshake key;
6. enforce source/subnet/global admission for new work;
7. only then invoke handshake/data crypto state machine.

Unknown random UDP traffic must not create one persistent object per source.

Add counters for cheap drops vs authenticated/protocol failures without logging raw packet payloads.

## 6. Runtime randomness/time fulfillment

Implement narrow adapters that supply:

- OS CSPRNG token values;
- ephemeral handshake key material where not already owned by crypto constructors;
- monotonic timestamps/deadlines;
- wall-clock protocol timestamps only where SSU2 requires wall time;
- retransmit/ACK scheduler wakeups.

Production runtime must never use deterministic seeded RNG.

Tests may inject deterministic sources at the protocol boundary or start the runtime with test-only constructors guarded from production composition.

## 7. Handshake registry and admission

For unauthenticated/pending handshakes:

- key state by the minimum safe tuple/connection identifier required by the spec;
- charge `PendingHandshakes` through existing `TransportResources` or a narrowly extended equivalent;
- enforce per-source/subnet/global caps;
- Retry/token requests should stay low-state where possible;
- pending state has absolute expiry;
- duplicate datagrams update only the matching bounded handshake state;
- successful authentication promotes atomically to active link/session admission;
- failure/drop/cancellation releases pending permits exactly once.

Do not clear dial backoff or declare a link active before peer authentication succeeds.

## 8. Generic transport-manager integration

On successful SSU2 authentication:

- register `TransportKind::Ssu2` with the existing `TransportManager`;
- reuse duplicate-link resolution rather than inventing SSU2-only peer ownership;
- active-link resource accounting must include SSU2;
- enqueue outbound I2NP delivery through the generic delivery contract;
- typed delivery outcomes remain transport-neutral;
- close/replacement/cancellation must remove the exact SSU2 link without disturbing a replacement link.

If UDP/message semantics require a small generic contract extension, make it transport-neutral and prove NTCP2 regressions remain green.

## 9. Outbound dial/session API

Provide a runtime API capable of:

```text
dial_ssu2(peer, validated RouterAddress, deadline)
send_i2np(peer/link, EncodedI2npMessage, deadline)
close_ssu2(link, reason)
```

Exact naming may follow existing NTCP2 runtime conventions.

Dial must:

- validate the address before socket activity;
- check shared/per-transport backoff;
- acquire pending admission;
- use TokenRequest/Retry or cached token according to protocol state;
- authenticate RouterInfo/peer identity;
- register active link only at the authenticated gate;
- return typed termination/dial outcomes.

## 10. Inbound I2NP handoff

Authenticated complete I2NP messages from Plan 157 must be emitted through the existing router/runtime dispatch seam, not delivered directly into NetDB/tunnel/client code from the SSU2 protocol crate.

For this plan, a narrow local test sink is acceptable if full production router dispatch is not active yet, but the ownership direction must match intended architecture:

```text
SSU2 runtime -> transport-neutral authenticated I2NP handoff -> caller/router dispatch
```

Do not special-case SAM or destinations into SSU2.

## 11. Central scheduler

Drive all SSU2 deadlines from one bounded runtime scheduler per service/socket set.

It must cover:

- handshake retries/deadlines;
- delayed ACK wakeups;
- loss/RTO wakeups;
- reassembly expiry;
- idle timeout;
- token expiry maintenance;
- termination cleanup.

Implementation may use one Tokio sleep repeatedly recomputed to the earliest deadline or another bounded wake mechanism.

Prohibit:

- one spawned sleep per packet;
- one task per retransmission;
- unbounded timer heap entries disconnected from live state.

## 12. Local real-UDP acceptance suite

Add focused black-box runtime tests that bind ephemeral loopback UDP ports and drive two independent i2pr SSU2 runtime instances through actual datagrams.

Required scenarios:

### 12.1 Tokenless establishment

- A has B's valid RouterInfo/address out-of-band;
- A starts without cached token;
- real Retry/token path occurs;
- SessionRequest/Created/Confirmed complete;
- both sides expose authenticated peer/link state;
- no private helper moves handshake bytes between protocol objects.

### 12.2 Cached-token establishment

- obtain a valid token through a legitimate first exchange;
- close first session cleanly;
- establish a subsequent session using the cached valid token path if allowed by current spec semantics;
- prove consumed/expired token behavior remains correct.

### 12.3 Bidirectional I2NP

- send multiple distinct encoded I2NP messages A→B and B→A;
- include small and fragmented messages;
- compare exact bytes/hashes at the authenticated handoff;
- do not require ordered delivery between distinct I2NP messages.

### 12.4 Loss/reorder

Use the narrowest runtime test seam that still preserves real UDP application boundaries. Preferred options:

- pre-send deterministic runtime datagram fault policy in test-only configuration;
- local UDP proxy owned by test code.

Do not bypass sockets by calling protocol-state `receive()` directly in the final local product test.

Prove single loss, ACK loss, reorder, duplicate, and recovery within resource ceilings.

### 12.5 Shutdown/resource baseline

- close sessions gracefully;
- abrupt peer shutdown;
- cancel runtime service with active sessions;
- all ChildScope tasks, pending handshakes, active links, queued datagrams, reassembly bytes and resource leases return to baseline.

## 13. Loopback policy and advertisement

During Plan 158:

- production default remains `enabled = false`;
- tests use loopback only;
- no public RouterInfo SSU2 address publication yet;
- `advertised = false` in support ledger;
- do not infer external reachability from local socket bind.

Plan 159 owns publication/reachability policy.

## 14. Likely files

```text
crates/i2pr-runtime/src/ssu2_runtime.rs
crates/i2pr-runtime/src/ssu2_driver.rs       # only if separation helps
crates/i2pr-daemon/src/config.rs
crates/i2pr-daemon/src/*composition*
crates/i2pr-runtime/tests/ssu2_local.rs
crates/i2pr-transport/src/*                  # minimal generic extension only
scripts/check-runtime-boundaries.sh
docs/architecture/i2pr-runtime.md
docs/architecture/i2pr-transport-ssu2.md
specs/support.toml
plans/158-status.md
```

Do not add SSU2 to production daemon startup until configuration explicitly enables it.

## 15. Validation

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
```

Run local real-UDP tests serially if port/scheduler behavior requires it; do not use arbitrary long sleeps to mask races.

## 16. Acceptance criteria

Plan 158 passes only when:

1. all production UDP sockets are owned by `i2pr-runtime`.
2. `i2pr-transport-ssu2` still has no Tokio/socket dependency.
3. `[ssu2]` is disabled by default and validated strictly.
4. receive loop cheaply rejects random/impossible traffic before avoidable session allocation/crypto.
5. pending handshakes and active sessions are resource-charged and bounded per source/global.
6. successful authentication atomically promotes through the generic `TransportManager` as `TransportKind::Ssu2`.
7. outbound delivery uses the existing transport-neutral I2NP delivery contract.
8. one central bounded scheduler owns handshake/ACK/loss/idle/reassembly deadlines.
9. no task/timer per packet architecture exists.
10. real localhost UDP tokenless establishment passes.
11. legitimate cached-token/reuse semantics are proven.
12. real localhost UDP bidirectional multi-I2NP exchange passes, including fragmentation.
13. real-socket fault tests recover from loss/reorder/duplicate within explicit bounds.
14. malformed/spoof/random datagrams do not create unbounded state.
15. cancellation/abrupt close returns tasks/resources/session tables to baseline.
16. no SSU2 public RouterInfo advertisement/reachability claim is introduced.
17. full workspace/SSU2 boundary/vector floor passes.
18. `plans/158-status.md` advances only to Plan 159.

## 17. Stop conditions

Stop and narrow if:

- UDP runtime integration requires bypassing `TransportManager` for normal authenticated delivery;
- correct scheduling appears to require task-per-packet/timer-per-packet;
- local real-UDP tests expose a Plan 156/157 protocol defect;
- IPv4 and IPv6 cannot share the service safely without explicit socket-family separation.

Fix the narrow layer that owns the defect rather than adding a runtime workaround.