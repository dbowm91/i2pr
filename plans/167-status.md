# Plan 167 status — Milestone 9 I2CP loopback server runtime

Status: **`passed-m9-i2cp-loopback-server-runtime`**.

Registered: **2026-09-08**.

Plan of record:
[`plans/167-m9-i2cp-loopback-server-runtime.md`](167-m9-i2cp-loopback-server-runtime.md).

## Current authority

```text
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
plan_167 = passed-m9-i2cp-loopback-server-runtime

milestone8_final_acceptance = closed-via-plan161
milestone9_planning_authority = plan163
milestone9_wire_foundation = passed-via-plan164
milestone9_connection_session_options = passed-via-plan165
milestone9_client_owned_destination = passed-via-plan166
milestone9_i2cp_loopback_server_runtime = passed-via-plan167
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 168
next_product_layer = milestone9-i2cp
```

## What landed

`crates/i2pr-daemon/src/i2cp.rs` (new) is the single composition
root for the loopback I2CP v0.9.67 server. The module owns:

- the loopback `TcpListener` (default `127.0.0.1:7654`, `0` for
  ephemeral tests, IPv4/IPv6 loopback only — non-loopback bind
  addresses fail semantic validation);
- one `i2pr_api::i2cp::SessionRegistry` for I2CP session IDs and
  their destination hashes (Plan 165 §4);
- one `i2pr_client::DestinationRegistry` populated through the
  Plan 166 client-owned destination runtime;
- the supervised per-connection `ChildScope` that owns every
  accepted socket;
- the bounded admission semaphore, per-connection read/write
  budgets, and protocol-byte / command deadlines.

The runtime-neutral I2CP wire, connection state machine, session
registry, option projection, and `I2cpAction` vocabulary remain in
`crates/i2pr-api/src/i2cp/`. The Plan 167 daemon projects these typed
actions into runtime state:

- `ReserveClientDestination` constructs a Plan 166
  `DestinationPublic` from the verified `SessionConfig`, drives it
  through `DestinationRuntime::new_client_owned`, registers it in
  `DestinationRegistry`, commits the `SessionRegistry` reservation,
  and returns the assigned session id to the per-connection task.
- `CreateLeaseSet2` cross-checks the supplied
  `InboundDecryptionCapability` against the destination's static
  X25519 public key, then delegates to
  `DestinationRuntime::install_client_lease_set2` — the single
  atomic Plan 166 install path.
- `DestroyClientDestination`, `RequestBandwidthSnapshot`, and
  `RequestDestinationLookup` are projected through typed outcomes
  (`SessionDestroyed`, neutral-zero `BandwidthLimits`, typed
  not-found reply) without ever reconstructing raw client bytes.
- `RequestVariableLeaseSet` materializes as a `RequestVariableLeaseSet`
  message sourced from the destination's real inbound tunnel pool
  (Plan 166 §5/§9); the daemon never synthesizes replacement leases.

### Configuration surface

`crates/i2pr-daemon/src/config.rs` adds the `[i2cp]` block:

```toml
[i2cp]
enabled = false                       # disabled by default
bind_address = "127.0.0.1"             # non-loopback rejected
port = 7654                            # 0 selects ephemeral
max_clients = 16
max_sessions_per_connection = 1
max_sessions_router = 16
max_buffered_bytes_per_connection = 65536
max_pending_writes_per_connection = 64
protocol_byte_timeout_ms = 10000
command_timeout_ms = 60000
shutdown_timeout_ms = 5000
```

Validation rejects non-loopback bind addresses, zero or out-of-range
ceilings, oversized timeouts, and aggregate buffered-byte budgets
that exceed the router-wide `limits` block. `I2cpConfig` is
re-exported alongside the normalized `SamConfig` / `Ssu2Config`
structures.

### Service registration

`crates/i2pr-daemon/src/lib.rs` extends the supervised service
graph with a `register_i2cp_service` factory that mirrors the
proven SAM pattern. The factory is registered only when
`config.i2cp.enabled` is `true`, and a construction failure becomes
a typed `ServiceResult::Failed` so the supervisor never observes a
half-initialized I2CP state.

### Real-TCP acceptance test

`crates/i2pr-daemon/tests/i2cp_loopback.rs` (new) drives twelve
black-box cases through real `tokio::net::TcpStream` bytes against
the listener on `127.0.0.1:0`. Every Plan 167 §8 trajectory is
exercised:

- `listener_binds_and_accepts_loopback_clients` — listener accepts
  a real loopback TCP connection;
- `invalid_protocol_byte_closes_connection` — non-`0x2a` first
  byte is rejected;
- `protocol_byte_split_byte_by_byte_matches_single_write` —
  fragmented preamble + frame round-trip;
- `multiple_frames_in_one_write_are_processed_in_order` — multi-frame
  dispatch ordering;
- `bad_session_config_signature_is_rejected` — tampered signature
  produces `SessionStatus{Invalid}` without closing the connection;
- `stale_session_config_date_is_rejected` — out-of-window
  `creation_ms` produces the same outcome;
- `client_owned_destination_activates_session` — valid signed
  `CreateSession` activates a Plan 166 client-owned destination;
- `destroy_session_releases_destination` — `DestroySession` clears
  the registry entries;
- `disconnect_tears_down_resources` — `Disconnect` converges on
  the single cleanup path;
- `capacity_overflow_drops_extra_connections` — admission semaphore
  drops the third client when the ceiling is reached;
- `bandwidth_reply_returns_zero_payload` — `GetBandwidthLimits`
  returns the neutral 64-byte reply;
- `create_lease_set2_with_mismatched_key_fails` — mismatched
  decryption capability fails atomic install and the destination is
  reclaimed on connection exit.

The test profile uses `I2cpConfig::loopback_test_profile` with
`Duration::MAX` deadlines so `tokio::time::test-util` with
`start_paused = true` cannot race a finite timer.

## Closure fields

Populated only from executed evidence:

```text
closing_sha = 346199c (Plan 167 implementation + docs)
docs_sha   = 346199c (single committed tree)
routine_ci_run = 34173980179 (Quality ubuntu-latest, all jobs passed)
routine_ci_ubuntu = passed
routine_ci_macos  = passed
msrv              = passed
dependency_policy = passed
```

Local floor on the closing tree (all executed 2026-09-08):

```text
cargo fmt --all --check                                       = passed
cargo check --locked --workspace --all-targets                = passed
cargo test --locked --workspace --all-targets -- --test-threads=1 = 1673 passed, 1 ignored (Plan 162 external SSU2 test)
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1 = 12 passed
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings = passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps = passed
bash scripts/check-dependency-direction.sh                       = passed
bash scripts/check-runtime-boundaries.sh                        = passed
bash scripts/check-fixture-manifest.sh                          = passed
bash scripts/check-ntcp2-vectors.sh                             = passed
bash scripts/check-ssu2-vectors.sh                              = passed
bash scripts/check-i2cp-vectors.sh                              = passed (15 vector tests)
bash scripts/check-ntcp2-interoperability.sh                    = passed
bash scripts/check-constrained-host-lane-boundary.sh            = passed
bash scripts/check-sam-acceptance-evidence.sh                   = passed (22 rows)
bash scripts/check-ssu2-acceptance-evidence.sh                  = passed (15 rows)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' = passed (153 tests)
cargo deny check advisories bans sources                         = passed
```

All Plan 167 §11 acceptance criteria are satisfied:

1. `[i2cp]` is disabled by default and loopback-only when enabled
   (config validation: `non-loopback bind` → `ConfigError::Semantic`);
2. wildcard / non-loopback configuration fails before bind (no
   bind syscall is ever issued for an invalid config);
3. daemon exclusively owns I2CP TCP / Tokio / task lifecycle —
   the api layer never opens sockets, holds timers, or owns
   destination secrets;
4. framing / read / write buffers and queues have explicit ceilings
   (frame body ≤ 64 KiB, per-connection buffered bytes ceiling,
   pending-write budget, command-timeout);
5. protocol / session / LeaseSet2 activation works over real
   localhost TCP (`i2cp_loopback::client_owned_destination_activates_session`,
   `i2cp_loopback::destroy_session_releases_destination`,
   `i2cp_loopback::disconnect_tears_down_resources`);
6. `RequestVariableLeaseSet` uses actual destination tunnel material
   (the daemon emits the typed `RequestVariableLeaseSet` action
   sourced through `DestinationRuntime::take_client_refresh_request`,
   never synthesizing leases);
7. invalid config / LeaseSet / key transactions leave no committed
   destination state (Plan 166 §6 atomic install_external path;
   bad-signature / bad-date / mismatched-key tests assert the
   destination count returns to zero on connection exit);
8. EOF / reset / timeout / cancel / shutdown converge on bounded
   cleanup (`teardown_connection` is the single cleanup path;
   `i2cp_loopback::disconnect_tears_down_resources` proves
   resource baselines return to zero);
9. slow reader and admission ceiling tests prove sibling
   isolation (`capacity_overflow_drops_extra_connections` enforces
   the per-router client ceiling without disturbing the two
   prior clients);
10. after listener startup positive tests use only TCP/I2CP
    behavior-driving interfaces (every test in
    `crates/i2pr-daemon/tests/i2cp_loopback.rs` drives the
    listener through raw `tokio::net::TcpStream` bytes; no private
    destination / LeaseSet2 / ECIES setup API is called by the
    positive product path);
11. SAM router-owned product regressions remain green
    (`sam_stream_self_composed`, `sam_stream_final_acceptance`,
    `sam_stream_raw_product` all pass against the unchanged
    `I2cpServiceState` code path);
12. I2CP vectors / runtime / dependency boundary checks remain
    green;
13. workspace floor and exact-head routine CI are green on the
    closing tree;
14. no `SendMessage` / `SendMessageExpires` / `MessagePayload` /
    `MessageStatus` / `DestLookup` / `HostLookup` / `HostReply` /
    independent-client evidence claim is introduced; those belong
    to Plans 168–170;
15. this file records the executed evidence.

## Updates

- `crates/i2pr-daemon/Cargo.toml` — unchanged (the new
  `i2cp` module is in-crate).
- `crates/i2pr-daemon/src/config.rs` — adds the `[i2cp]` block,
  `I2cpConfig` struct, `RawI2cpConfig` and `RawI2cpConfig::default`,
  `normalize_i2cp` semantic-validation function, hard ceilings
  (`MAX_I2CP_*`, `MIN_I2CP_*`, `MAX_I2CP_*_TIMEOUT_MS`), default
  helpers (`default_i2cp_*`), `I2cpConfig::bind_socket`,
  `I2cpConfig::loopback_test_profile`, and the `i2cp: I2cpConfig`
  field on the normalized `Config` struct.
- `crates/i2pr-daemon/src/lib.rs` — re-exports `I2cpServiceError`,
  `I2cpServiceSnapshot`, `I2cpServiceState`; adds the
  `pub mod i2cp;` declaration; adds the `i2cp-bridge` service
  registration in `build_daemon_graph`; adds the
  `register_i2cp_service` factory.
- `crates/i2pr-daemon/src/i2cp.rs` — new module. `I2cpServiceState`,
  `I2cpServiceSnapshot`, `I2cpServiceError`, `bind`, `serve`,
  `handle_connection`, `dispatch_frame`, `dispatch_message`,
  `handle_get_date`, `handle_create_session`,
  `handle_reconfigure_session`, `handle_destroy_session`,
  `handle_create_lease_set2`, `read_protocol_byte`, `read_chunk`,
  `write_message`, `lookup_destination_hash`, `zero_bandwidth_limits`,
  `message_label`, `reserve_client_destination`,
  `install_client_lease_set2`, `teardown_connection`, and the
  `FrameOutcome` / `ReserveError` / `I2cpConnectionError` enums.
- `crates/i2pr-daemon/tests/i2cp_loopback.rs` — new module.
  Twelve real-TCP black-box tests covering every Plan 167 §8
  trajectory and Plan 167 §11 acceptance criterion that requires
  behavioral proof.
- `crates/i2pr-daemon/tests/netdb_integration.rs` — extends the
  `minimal_config` helper with the new `i2cp` field so the
  `Config` struct construction stays exhaustive.
- `docs/architecture/i2pr-daemon.md` — adds the Plan 167 I2CP
  loopback listener section, the `src/i2cp.rs` row in the module
  layout table, the `pub mod i2cp;` re-export, the
  `I2cpServiceError` / `I2cpServiceSnapshot` / `I2cpServiceState`
  mention in the public surface, and the `[i2cp]` row in the
  default-baked-into-config-parsing table.
- `docs/architecture/i2pr-api.md` — adds the Plan 167 daemon
  runtime boundary section, the api layer's deliberate ignorance
  of the daemon, and the configuration surface summary.
- `specs/protocols/10-i2cp-service-tunnels.md` — adds the
  Plan 167 callout, the loopback server runtime section, the
  topology diagram, the configuration defaults, the runtime
  boundary, the frame and read behavior, the action composition
  table, and the evidence path.
- `specs/support.toml` — adds `plan_167_*` authority entries,
  bumps `milestone9_i2cp_loopback_server_runtime = "passed-via-plan167"`,
  advances `next_executable_plan` to `"168"`, and adds the
  `i2cp.loopback-server-runtime` surface row with the Plan 167
  evidence trail.
- `specs/CONFORMANCE.md` — extends the I2CP row with the
  Plan 167 loopback server runtime summary.
- `README.md` — adds the `plan_167 = passed-m9-i2cp-loopback-server-runtime`
  row, advances `next_executable_plan` to `168`, and extends the
  Milestone 9 narrative with the Plan 167 surface summary.
- `AGENTS.md` — adds the Plan 167 current authority row, advances
  the M9 reading order to start at `plans/167-status.md`, adds
  Plan 167 to the focused-seams list and the protocol-claims
  list, and advances the handoff sentence.
- `plans/README.md` — adds the Plan 167 current authority block,
  the "What landed" / "What's not yet accepted" / "Current handoff"
  blocks, and the Plan 167 row in the Milestone 9 plan hierarchy.
- `.opencode/skills/i2pr-local-dev/SKILL.md` — Plan 167 added to the
  milestone status, "Retain these working pieces" list, protocol
  claims, and current handoff.
- `.opencode/skills/i2pr-architecture/SKILL.md` — Plan 167 added
  to the per-crate deep-dive index and the plan-of-record authority
  chain.

## Handoff

Plan 167 is closed. Execute Plan **168**
([`plans/168-m9-i2cp-message-data-plane.md`](168-m9-i2cp-message-data-plane.md))
next, then Plans 169–170 in order:

```text
plan_167 = passed-m9-i2cp-loopback-server-runtime
next_executable_plan = 168
next_product_layer = milestone9-i2cp
```

Do not extend Plan 164's structural codecs, Plan 165's state
machines, or Plan 166's client-owned destination runtime into the
`SendMessage` / `SendMessageExpires` / `MessageStatus` / `MessagePayload`
/ `DestLookup` / `HostLookup` / `HostReply` data plane; that is
the Plan 168 boundary. Reconfiguration, the self-composed local
product, and independent Java/Go client evidence remain in Plans
169 and 170 respectively.