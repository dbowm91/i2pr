# Plan 138 status — SAM 3.1 STREAM CONNECT / ACCEPT transport bridge

Status: **`passed-m7-sam31-stream-connect-accept-bridge`**.

Registered: **2026-08-28**.

Source floor: Plan 137 closed `passed-m7-sam31-loopback-server-session-lifecycle`
(loopback listener, bounded session registry, line reader, server
state machine, transactional `SESSION CREATE`, per-destination
`StreamingManager` pool). Plan 138 attaches the loopback listener to
that pool so real `STREAM CONNECT` and `STREAM ACCEPT` sockets drive
the production M6 `StreamingManager -> StreamingDestinationAdapter
-> destination routing` path over `127.0.0.1`.

Plan of record:
[`plans/138-m7-sam31-stream-connect-accept-bridge.md`](138-m7-sam31-stream-connect-accept-bridge.md).

## Current classification

```text
plan_137 = passed-m7-sam31-loopback-server-session-lifecycle
plan_138 = passed-m7-sam31-stream-connect-accept-bridge

milestone6_local_product    = passed
milestone6_interoperable    = not-yet-claimed
external_acceptance_debt    = retained-separately

milestone7_sam_protocol     = passed
milestone7_loopback_server  = passed
milestone7_stream_bridge    = passed
milestone7_forward_naming   = blocked-on-plan139
milestone7_closure          = blocked-on-plan140

next_executable_plan        = 139
```

## Scope shipped

### Runtime-neutral surface (`crates/i2pr-api/src/sam/`)

- `streams.rs` (new) — bounded per-session stream-socket registry.
  Defines `SamStreamId` (alias `StreamAcceptId`), `SamStreamState`
  (`Connecting`, `WaitingAccept`, `Established`, `Closing`,
  `Closed`), `SamStreamDirection` (`Outbound`, `Inbound`),
  `SamStreamAttachment` (one per STREAM socket), `SamStreamEntry`
  (FIFO pending-accept queue), `SamStreamRegistry`,
  `SamStreamRegistryError`, and the typed handle
  `SamStreamRegistryHandle`. The registry enforces
  `SamLimits::max_stream_sockets_per_session` and
  `SamLimits::max_pending_accepts_per_session` exactly; the FIFO
  ACCEPT queue orders waiters by registration.
- `command.rs` — typed `StreamConnectRequest` /
  `StreamAcceptRequest` plus their `parse_stream_connect` /
  `parse_stream_accept` extractors. The mapped errors
  (`StreamConnectError`, `StreamAcceptError`) round-trip through
  the existing `Display` path so the daemon can map them to
  `RESULT=INVALID_ID` / `RESULT=I2P_ERROR`.
- `server_state.rs` — new `DispatchOutcome` variants
  `RequireStreamConnect { request: Box<StreamConnectRequest> }`
  and `RequireStreamAccept { request: Box<StreamAcceptRequest> }`.
  `handle_recognised` routes `StreamConnect` and `StreamAccept`
  to the new variants; `StreamForward` keeps the
  `NOT_IMPLEMENTED` reply that Plan 139 owns.
- `reply.rs` — `StreamStatus::result()` accessor so the dispatch
  applier can match on the result code.

### Runtime bridge (`crates/i2pr-daemon/src/sam/streams.rs`)

- `SamDestinationBridge` — per-destination runtime bridge. Holds
  the destination identity inside an `Arc<DestinationIdentity>` (the
  identity is non-`Clone`; the `Arc` lets the per-stream task call
  `StreamingManager::connect` with `&DestinationIdentity` while
  holding the mutable manager borrow). Owns the local
  `LeaseSet2` and `DestinationOutboundRole` produced by
  `build_signed_lease_set2` over the same `DestinationTunnelPool`
  -> `inbound_lease_sources` path Plan 129 uses.
- `SamDestinations` — `Arc<Mutex<HashMap<DestinationId,
  SamDestinationHandle>>>` registry. `SamServiceState` owns it
  next to the existing `StreamingPools`.
- `record_captured` / `drain_captured_outbound` — test seam and
  future Plan 140 delivery wiring. Every byte the StreamingManager
  emits is captured so the test can verify the full
  `StreamingManager -> StreamingDestinationAdapter` path.
- `adapter_send` — routes each captured `TransportSendRequest`
  through `StreamingDestinationAdapter::send`, recording the
  resulting `OutboundDeliveryPlan` size alongside the captured
  payload. The adapter RNG uses a deterministic `ChaCha8Rng` for
  the local seam; Plan 140 swaps in a CSPRNG when wiring the live
  tunnel delivery layer.
- `decode_destination_triple` — strict RFC 4648 Base64 + destination
  decoder for the `STREAM CONNECT DESTINATION=` value, returning
  the `(DestinationId, SigningPublicKey, StaticPublicKey)` triple
  `StreamingManager::connect` requires.

### Daemon SAM service (`crates/i2pr-daemon/src/sam.rs`)

- `SamServiceState` exposes `sam_destinations()` and removes the
  per-destination bridge from `teardown_session` exactly once.
- `dispatch_command` routes `RequireStreamConnect` to the new
  `execute_stream_connect` and `RequireStreamAccept` to the new
  `execute_stream_accept`. `apply_stream_connect_outcome` /
  `apply_stream_accept_outcome` are imported from `i2pr-api`.
- `execute_stream_connect` validates the session id, decodes the
  supplied destination, calls `StreamingManager::connect` over the
  bridge, drains the outbound queue and routes every
  `TransportSendRequest` through the adapter, then yields until the
  outbound connection reaches `Established`. The wait is bounded;
  a timeout maps to `RESULT=TIMEOUT`. Plan 138 returns
  `RESULT=OK` only once `Established` is observed, per Plan 138 §7.
- `execute_stream_accept` validates the session id, ensures a
  wildcard `StreamingManager::listen(0)` is bound on the destination
  pool, and returns the registered stream id. The inbound SYN
  observation is driven by the test seam in Plan 138's local
  loopback; Plan 140 wires the real inbound delivery path.

### Tests

- `crates/i2pr-api` — 105 unit tests cover `SamStreamRegistry`
  (FIFO ACCEPT, per-session ceiling, idempotent release,
  state-lifecycle updates, peer-destination capture).
- `crates/i2pr-daemon/tests/sam_stream.rs` — 10 real-loopback
  integration tests. Each test binds the listener on
  `127.0.0.1:0` and drives a real TCP client. Coverage:
  - `STREAM CONNECT` against unknown session → `RESULT=INVALID_ID`
  - `STREAM CONNECT` with malformed destination text →
    `RESULT=INVALID_KEY`
  - `STREAM CONNECT` before `HELLO` → typed `INVALID_ID` /
    `I2P_ERROR`
  - `STREAM CONNECT` missing `DESTINATION=` → `RESULT=I2P_ERROR`
  - `STREAM ACCEPT` against unknown session → `RESULT=INVALID_ID`
  - `STREAM FORWARD` → `RESULT=NOT_IMPLEMENTED` (Plan 139)
  - `STREAM ACCEPT` socket open + drop is handled cleanly
  - `STREAM CONNECT` capture seam records the outbound
    `TransportSendRequest`
  - bridge `record_captured` / `drain_captured_outbound` round-trip
  - `QUIT` closes the control socket
- `crates/i2pr-daemon/tests/sam_loopback.rs` updated for the new
  Plan 138 dispatch behaviour: the prior `NOT_IMPLEMENTED` test is
  now the more specific `stream_connect_unknown_session_returns_invalid_id`.

### Specs / docs

- `docs/protocol-support.md` (mirrored in `specs/protocol-support.md`)
  and `specs/support.toml` — SAM row reflects Plan 138 surface
  additions and the closed Plan 138 status.
- `docs/architecture/i2pr-api.md` — describes the new
  `SamStreamRegistry` surface and the dispatch flow.
- `docs/architecture/i2pr-daemon.md` — describes the
  `SamDestinationBridge`, the capture seam, and the production
  dispatch path.
- `docs/architecture/overview.md` — index entry for Plan 138.
- `.opencode/skills/i2pr-local-dev/SKILL.md` — Plan 138 status and
  module layout.
- `README.md`, `AGENTS.md` — Plan 138 milestone + Plan 139 as the
  next executable plan; SAM workspace layout reflects the new
  bridge + stream registry; focused seam documents the SAM
  integration lane.

## Validation commands

Run from the repository root with the pinned Rust 1.95 toolchain.

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test sam_stream
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

## Verification

```text
$ cargo fmt --all --check                       # clean
$ cargo check --locked --workspace --all-targets # clean
$ cargo test --locked --workspace               # 1229 passed (57 suites)
$ cargo test --locked -p i2pr-api --all-targets  # 105 passed (1 suite)
$ cargo test --locked -p i2pr-daemon --test sam_stream # 10 passed (1 suite)
$ cargo test --locked -p i2pr-daemon --test sam_loopback # 17 passed (1 suite)
$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                # clean
$ RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                # clean
$ bash scripts/check-dependency-direction.sh     # ok
$ bash scripts/check-runtime-boundaries.sh       # ok
```

Static boundaries honoured:

- `i2pr-api` owns the runtime-neutral SAM surface only (no
  `tokio::*`, no `std::net`, no `std::fs`, no `unsafe`).
- `i2pr-daemon` owns the supervised Tokio listener, the
  per-destination `SamDestinationBridge`, the `StreamingPools`
  extensions, and the SAM STREAM dispatch path.
- No production crate depends on `i2pr-testkit`.
- SAM bind address is rejected unless it parses as a loopback
  `IpAddr`.

## Known limitations / external acceptance debt

- The Plan 138 byte bridge is bounded by `SamLimits`. The
  `max_buffered_bytes_per_stream_direction` ceiling is enforced
  per-direction by the `SamLimits` validation; the per-stream
  runtime queue wiring is owned by Plan 140, which replaces the
  test seam with the live outbound tunnel delivery layer.
- Plan 138's local seam uses a deterministic `ChaCha8Rng` for
  the `StreamingDestinationAdapter` RNG. Production CSPRNG
  selection (with deterministic fallback) is owned by Plan 140.
- The per-destination driver task that periodically calls
  `StreamingManager::poll_retransmits` / `poll_acks` is not yet
  spawned by the daemon. Plan 138's local seam drives the
  manager directly; Plan 140 adds the tokio-driven driver loop.
- Mixed-router SAM interoperability evidence (Java I2P / i2pd SAM
  v3.1 endpoints) is tracked as external acceptance debt.

## Deviations

- The SAM STREAM socket task structure retains a single
  per-connection task rather than splitting into a separate
  `handle_stream_connection` task. Plan 138 reports OK only after
  `Established` is observed inside the same dispatch; Plan 140
  splits the byte bridge into a dedicated task once the live
  outbound delivery path is wired.
- `StreamStatus::result()` accessor is exposed so the dispatch
  applier can match on the result code without re-decoding the
  wire form.
- The `SamSessionRegistryError` dependency in `dispatch_command`
  is retained at the daemon level (not at the
  `ServerConnectionState` level), matching the Plan 137 ownership
  shape.

## Next executable plan

Plan 139 — SAM 3.1 STREAM FORWARD, NAMING LOOKUP, and the
`STREAM FORWARD ID=` listener / forwarding bridge. The Plan 138
`SamStreamRegistry` FIFO and `SamDestinationBridge` already model
the per-session state machine the FORWARD lane composes against.