# Plan 137 status — SAM 3.1 loopback server and session lifecycle

Status: **`passed-m7-sam31-loopback-server-session-lifecycle`**.

Registered: **2026-08-28**.

Source floor: Plan 136 closed `passed-m7-sam31-protocol-private-destination-foundation`
(local SAM 3.1 parser, typed version model, Base64 codec, and
typed private-destination surface). Plan 137 builds the runtime
loopback service graph on top of that surface; the production
daemon never advertises or activates SAM v3.1 (`enabled = false`
by default).

Plan of record:
[`plans/137-m7-sam31-loopback-server-session-lifecycle.md`](137-m7-sam31-loopback-server-session-lifecycle.md).

## Current classification

```text
plan_136 = passed-m7-sam31-protocol-private-destination-foundation
plan_137 = passed-m7-sam31-loopback-server-session-lifecycle

milestone6_local_product    = passed
milestone6_interoperable    = not-yet-claimed
external_acceptance_debt    = retained-separately

milestone7_sam_protocol     = passed
milestone7_loopback_server  = passed
milestone7_stream_bridge    = blocked-on-plan138
milestone7_forward_naming   = blocked-on-plan139
milestone7_closure          = blocked-on-plan140

next_executable_plan        = 138
```

## Scope shipped

### Runtime-neutral surface (`crates/i2pr-api/src/sam/`)

- `limits.rs` — `SamLimits` struct, validated bounds, and
  `loopback_test_profile` helper. Validation rejects zero fields
  and finite fields that exceed documented ceilings
  (`MAX_SAM_CLIENTS`, `MAX_SAM_SESSIONS`,
  `MAX_SAM_STREAM_SOCKETS_PER_SESSION`,
  `MAX_SAM_PENDING_ACCEPTS_PER_SESSION`,
  `MAX_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION`,
  `MAX_SAM_*_TIMEOUT_SECS`). `Duration::MAX` is the documented
  sentinel that disables the per-read timeout used by integration
  tests that drive the listener under `tokio::time::test-util`.
- `session.rs` — `SamSessionId` newtype, `SamSessionCounters`,
  and `SamSessionCountersError`. `SamSessionId::new` rejects
  empty strings, oversize strings, and bytes outside the
  conservative SAM-safe alphabet.
- `registry.rs` — `SamSessionRegistry`, `SamSessionEntry`,
  `SamSessionReservation`, `ControlOwnerState`, and the
  `reserve_session` / `commit_reservation` /
  `rollback_reservation` triplet that makes the SAM-and-destination
  insert transactional. Two-step insertion guarantees the registry
  never observes a half-built session.
- `line_reader.rs` — `LineReader` with `push`, `push_one`, and
  `LineEvent::{CompleteLine, OverflowLine, ControlByteInLine,
  NeedMore}`. Enforces the documented line byte ceiling, rejects
  control bytes (configurable), and preserves post-newline bytes
  via `append_unchecked` so multi-line writes are dispatched one
  at a time.
- `server_state.rs` — `ServerConnectionState`
  (`AwaitHello`/`UtilityReady`/`SessionControl`/`Closed`),
  `DispatchOutcome`, `CloseReason`, `dispatch`, and
  `apply_session_outcome`. HELLO negotiates exactly 3.1,
  DEST GENERATE stays on `UtilityReady` (the daemon fills in the
  Base64), SESSION CREATE returns `RequireSessionCreate` so the
  daemon can run the transactional destination insert, QUIT/STOP/EXIT
  close the connection, PING echoes the payload, and STREAM /
  NAMING commands reply `NOT_IMPLEMENTED` per Plan 137 §6.
- `parser.rs` — PING captures every token after `PING` so
  multi-token payloads round-trip through `PONG`.
- `reply.rs` — `SessionStatus::result()`, `DestReply::ok_pub_priv`,
  and `DestReply::ok_public` constructors. Reply line produced
  through `Reply::Session(...)` + `.encode()`.

### Supervised loopback service (`crates/i2pr-daemon/src/sam.rs`)

- `SamServiceState` owns the loopback `TcpListener` configuration,
  the bounded `SamSessionRegistry`, the underlying
  `i2pr_client::DestinationRegistry`, the per-destination
  `i2pr_client::streaming::StreamingManager` pool that Plan 138
  will attach STREAM sockets to, the destination runtime
  configuration, and the supervised child scope that owns every
  per-connection task.
- `run` / `bind` / `serve` — `bind` exposes the bound socket
  address so integration tests and the daemon composition root
  can observe the actual port before the listener enters the
  accept loop. `serve` enforces the per-client semaphore
  (`max_clients`), honours the `child_token` cancellation
  propagation, and emits `INFO`/`WARN`/`DEBUG` log lines for
  bind / accept / cleanup.
- `handle_connection` — Tokio-driven loop that reads lines via
  the runtime-neutral `LineReader`, dispatches every command
  through `server_state::dispatch`, writes the typed reply, and
  enforces the `Duration::MAX`-disabled per-read timeout. The
  loop is `select!`-wrapped on the per-task and service
  cancellation tokens so a `parent.cancel` reaches every active
  per-socket task immediately.
- `execute_session_create` — three-step transactional insert
  (`reserve_session` → `DestinationRegistry::insert` →
  `StreamingPools::install` → `commit_reservation`). Any failure
  rolls back the prior step via `rollback_reservation` or
  `teardown_session`. Imported `SamPrivateDestination` and
  TRANSIENT both round-trip through
  `SamPrivateDestination::encode_public_base64` so the SAM reply
  carries the canonical public text.
- `teardown_session` — idempotent cleanup of session, destination,
  and streaming pool entries. Called from the per-socket cleanup
  path and from the supervisor shutdown path.
- `map_registry_error` / `map_destination_registry_error` — typed
  mapping of `SamSessionRegistryError` / `RegistryError` into the
  SAM `ReplyResult` vocabulary. Duplicate session IDs become
  `DuplicatedId`; duplicate destinations become
  `DuplicatedDestination`; capacity exhaustion becomes `I2P_ERROR`.

### Daemon composition (`crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-daemon/src/config.rs`)

- New `[sam] enabled = false | true`, `bind_address`, `port`,
  `limits` config section. `normalize_sam` validates the section,
  enforces loopback-only binding (non-loopback addresses are
  rejected), defaults `bind_address = 127.0.0.1`, and disables
  the listener by default so the production daemon stays
  unmodified.
- `register_sam_service` is wired into `build_daemon_graph` when
  `config.sam.enabled = true`. The service uses
  `ServiceFailureCategory::InvalidState` for binding failures and
  wraps the bound-address string via `HealthDetail::new(...).ok()`
  per Plan 137 §9.
- `Config::default_for_test` / `Config::default_for_test_with_data_dir`
  integration-only helpers expose a `Config` whose `[sam]`
  section is disabled by default. `tests/netdb_integration.rs`
  was updated to pass an explicit `sam: SamConfig { enabled: false,
  ... }` so the `Config` shape stays explicit.
- `crate::sam` module is `pub` and re-exports `SamServiceError`,
  `SamServiceState`, and `StreamingPools` for downstream
  composition and integration tests.

### Dependency direction

- `i2pr-daemon` now depends on `i2pr-api` and `i2pr-client`.
  `scripts/check-dependency-direction.sh` was extended with the
  exact `i2pr-daemon -> {i2pr-api, i2pr-client, ...}` allowlist.

### Integration lane (`crates/i2pr-daemon/tests/sam_loopback.rs`)

- 17 integration tests, all `#[tokio::test(flavor = "current_thread",
  start_paused = true)]`. Each test binds the listener on an
  ephemeral port, exercises a real TCP path, and asserts the
  typed wire reply. Coverage:
  - listener bind / accept / version negotiation
  - HELLO with incompatible version → close
  - SESSION CREATE before HELLO → reject
  - DEST GENERATE typed reply, no session count increase
  - SESSION CREATE TRANSIENT round-trip
  - SESSION CREATE with imported private destination
  - duplicate session id rejected
  - second SESSION CREATE on the same control socket rejected
  - control disconnect tears down session and destination
  - PING echoes the full multi-token payload
  - STREAM CONNECT / ACCEPT / FORWARD return `NOT_IMPLEMENTED`
    per Plan 137 §6
  - session capacity boundary enforced (`max_sessions = 16` via
    `SamLimits::loopback_test_profile`)
  - HELLO byte-by-byte matches single write
  - sequential PING after HELLO
  - QUIT after HELLO closes the control socket
  - service shutdown closes the listener and active clients

### Runtime seam (`crates/i2pr-runtime/src/context.rs`)

- New `ChildScope::for_test(parent, policy)` helper that
  exposes the `pub(crate)` `ChildScope::new` to integration
  tests via a `TaskCounters::new()` handle. Tests that exercise
  `i2pr-daemon` directly do not need to depend on
  `i2pr-runtime::observability` (which stays private).

## Verification

```text
$ cargo fmt --all -- --check              # clean
$ cargo check --workspace --all-targets   # clean
$ cargo test --workspace                  # 650 passed; 0 failed
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
                                            # clean
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
                                            # clean
$ bash scripts/check-dependency-direction.sh   # ok
$ bash scripts/check-runtime-boundaries.sh     # ok
$ bash scripts/check-fixture-manifest.sh        # ok
$ bash scripts/check-ntcp2-interoperability.sh  # ok
$ bash scripts/check-ntcp2-vectors.sh           # ok
$ bash scripts/check-multipass-interop-boundary.sh
                                              # ok
```

Static boundaries honoured:

- `i2pr-api` owns the runtime-neutral SAM surface only
  (no `tokio::*`, no `std::net`, no `std::fs`, no `unsafe`).
- `i2pr-daemon` owns the supervised Tokio listener, the
  per-destination streaming pool, and the SAM→daemon composition.
- No production crate depends on `i2pr-testkit`.
- SAM bind address is rejected unless it parses as a loopback
  `IpAddr`.

## Known limitations / external acceptance debt

- No external SAM router interoperability evidence has been
  collected. Plan 137 is a **local** loopback service that
  exercises the SAM 3.1 wire protocol against the daemon's own
  client. Mixed-router evidence (Java I2P / i2pd SAM v3.1
  endpoints) is tracked as external acceptance debt.
- `STREAM CONNECT` / `STREAM ACCEPT` / `STREAM FORWARD` are
  wired through `server_state::dispatch` but reply
  `NOT_IMPLEMENTED` and never enter the per-destination
  `StreamingManager` pool. Plan 138 owns the transport bridge.
- `NAMING LOOKUP` is similarly stubbed with `NOT_IMPLEMENTED`.
  Plan 139 owns the forward-naming and LS2 publication lane.
- The per-read timeout uses `tokio::time::timeout`, which races
  with `tokio::time::test-util` auto-advance. Integration tests
  opt out of the deadline via `SamLimits::loopback_test_profile`
  (`Duration::MAX` sentinel). Production callers use the
  finite `SamLimits::defaults()` ceilings.
- The production daemon defaults to `[sam] enabled = false`. No
  independent deployment has yet advertised or relied on the
  SAM 3.1 surface.

## Deviations

- `RegistryConfig` is constructed with `max_destinations = max_sessions`
  so the router-local destination pool never underflows the SAM
  session ceiling. The Plan 137 default `RegistryConfig::default`
  (4 destinations) would have caused the `session_capacity_boundary`
  integration test to fail with a typed
  `RegistryError::CapacityExceeded`.
- `ChildScope::for_test` was added instead of broadening the
  visibility of `ChildScope::new`, preserving the runtime
  encapsulation contract from Plan 119.
- `Duration::MAX` is the documented sentinel that disables the
  per-read timeout. Validation explicitly allows the sentinel
  while still rejecting zero / oversize finite durations.

## Next executable plan

Plan 138 — SAM 3.1 STREAM CONNECT / ACCEPT / FORWARD transport
bridge. The Plan 137 per-destination `StreamingManager` pool is
the composition seam Plan 138 attaches sockets to.
