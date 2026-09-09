# Plan 170 status — Milestone 9 I2CP independent clients and final closure

Status: **`passed-m9-i2cp-independent-clients-and-final-closure`
(local lane evidence complete; commit + hosted CI pending — see §8).**

Registered: **2026-09-08**.

Plan of record:
[`plans/170-m9-i2cp-independent-clients-and-final-closure.md`](170-m9-i2cp-independent-clients-and-final-closure.md).

## Current authority

```text
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_170 = passed-m9-i2cp-independent-clients-and-final-closure

milestone9_planning_authority = plan163
milestone9_protocol = i2cp
milestone9_i2cp_wire_foundation = passed-via-plan164
milestone9_i2cp_connection_session_options = passed-via-plan165
milestone9_i2cp_client_owned_destination = passed-via-plan166
milestone9_i2cp_loopback_server_runtime = passed-via-plan167
milestone9_i2cp_message_data_plane = passed-via-plan168
milestone9_i2cp_self_composed_local_product = passed-via-plan169
milestone9_i2cp_independent_clients = passed-via-plan170
milestone9_final_acceptance = closed-via-plan170
next_product_layer = milestone10-planning
```

## What landed

### Daemon compatibility deltas (all in service of unmodified clients)

`crates/i2pr-daemon/src/i2cp.rs`:

- `ReplyAndFollowup` write path: after `SessionStatus(Created)`,
  the daemon immediately emits the `RequestVariableLeaseSet`
  (empty leases) follow-up the unmodified Java I2P
  `I2PSessionImpl.connect()` wait progresses on without tunnels.
- Cross-session loopback shortcut retained verbatim from Plan 168
  (no private injection seam: sender `SendMessage` bytes are
  forwarded opaquely into the receiving session's bounded inbound
  queue and drained as `MessagePayload` onto its TCP stream).
- No debug logging in production source: the lane gates on
  driver-emitted facts, not daemon stderr.

`crates/i2pr-api/src/i2cp/connection.rs`:

- `validate_version` accepts any well-formed `0.x.y` client and
  answers with the M9 advertised version (`0.9.67`) in `SetDate`.
  Java I2P 2.13.0 sends `0.9.70`, go-i2cp sends `0.9.67`; a
  non-zero major is still rejected as unnegotiable
  (`unknown_version_is_rejected` covers `1.2.3`).
- Empty GetDate authentication mappings (I2CP 0.9.11+ compliance,
  sent by both clients) are accepted; non-empty credentials are
  still rejected with `AuthNotSupported`.

`crates/i2pr-api/src/i2cp/config.rs`:

- `messageReliability=none` (shipped by default by both Java
  `I2PSessionImpl` and go-i2cp) maps to the M9 best-effort
  posture with a documented ignored note instead of
  `OptionRejected`.

`crates/i2pr-api/src/i2cp/verify.rs` + `crates/i2pr-client/src/identity.rs`
(Plan 170 §6 policy relocation):

- The Destination encryption-key slot is an I2P legacy field
  (unused since 2005) that Java I2P 2.13.0, go-i2cp, and i2pd all
  populate with ElGamal-2048 even for X25519 LeaseSet2 sessions.
  SessionConfig verification and `DestinationPublic` accept the
  legacy slot (zeroed static slot); X25519 enforcement lives at
  the Plan 166 `install_client_lease_set2` capability↔LS2 match,
  which stays fail-closed for legacy-slot sessions
  (`DecryptionCapabilityKeyMismatch`). Unit tests were rewritten
  to pin both halves rather than deleted.

### External drivers (non-production, unmodified libraries only)

- `tests/integration/i2cp/external/java/i2cp_java_driver.java`:
  drives unmodified `net.i2p.data.i2cp` wire primitives
  (`I2PSession.connect()` bypassed — it blocks on real tunnels
  the M9 profile skips; documented deviation). Full-gzip payload
  emit (header + deflate + CRC-32/ISIZE trailer, LE ports matching
  go-i2cp's extraction), inflate-on-receive with application-byte
  digest, corrected MessageStatus field offsets (status=body[6],
  nonce=body[11..15]), plus `bandwidth` (GetBandwidthLimits),
  `connect-version`, `session-leaseset2`, `cleanup` modes.
- `tests/integration/i2cp/external/go/i2cp_go_driver.go`:
  public go-i2cp API only (`CreateSessionSync` + own `ProcessIO`
  loops, which `CreateSessionSync` cancels on return — the
  receiver was missing its loop, so `OnMessage` never fired).
  Records inbound digests, `delivery_path=client_parsed_digest`
  on parse, MessageStatus facts on send.

### Lane, checker, workflow

- `tests/integration/i2cp/run-independent.sh`: 9 command-derived
  rows, fail-closed pins/cache guards, per-row digest/length/port
  gates, §13 bounded retry (3 attempts, fresh sessions each,
  attempt recorded; protocol failures are never retried green).
- `scripts/check-i2cp-acceptance-evidence.sh`: 9-row integrity
  gate (no literal passes, exit-code gating, pins, loopback-only,
  digest-equality + strong-parse-path requirements), enforced in
  routine Linux CI per the plan.
- `.github/workflows/i2cp-external.yml`: manual Ubuntu lane
  (ant/JDK/Go install, fetch, checker, matrix, evidence upload).
- `scripts/interop/fetch-i2cp-clients.sh`: exact pins, hash
  verification, unmodified builds.
- `crates/i2pr-daemon/examples/i2cp_loopback_listener.rs`:
  loopback test-profile listener used by the lane.

## Evidence (local, two consecutive runs, same tree)

Pins: Java I2P 2.13.0 `9134f808337b401e8e53c73734c81fab04280c9d`,
go-i2cp `b529ee1c10a6011558b4d69fc9436a4afc489eac`.
Bind: `127.0.0.1` only. No root/namespaces/Docker/VM/systemd.

```text
bash tests/integration/i2cp/run-independent.sh   # passed twice
bash scripts/check-i2cp-acceptance-evidence.sh   # 9 rows command-derived
```

Final run rows (all `passed`, all attempt 1/3):

```text
external-java-to-go-small    digest f5cd0143d29d... (25 B each side)
external-java-to-go-large    digest d8690a426100... (32768 B each side)
external-go-to-java-small    digest f5cd0143d29d... (25 B each side)
external-go-to-java-large    digest d8690a426100... (32768 B each side)
external-message-status-semantics  (session+nonce 42 accept, all four runs)
external-bandwidth-query     (16 limits, client 64/64 KB/s, router 0/0)
plan167-169-focused-regressions
workspace-gates
external-clean-resource-baseline
```

Digest proof (small, both directions identical):

```text
java outbound sha = go inbound sha = f5cd0143d29dcb89b10a5b94f70a5143a9731a82e3d9c86f9e4517f3fe81a3cc
go outbound sha = java inbound sha = f5cd0143d29dcb89b10a5b94f70a5143a9731a82e3d9c86f9e4517f3fe81a3cc
```

Ports `7/8`, protocol `6` matched on every parsed inbound.

Full floor on the closing tree: `cargo fmt --check`, `cargo check
--locked --workspace --all-targets`, `cargo test --locked --workspace
--all-targets` (1728 passed, 1 ignored, 68 suites), `cargo clippy
--all-targets --all-features -D warnings`, `cargo doc -D warnings`,
doc tests, all eleven static boundary/vector/evidence scripts,
`python3 -m unittest discover` (153 OK), `cargo deny check
advisories bans sources` — all green.

## Known limitations (explicit, non-blocking)

- Java driver uses raw wire primitives, not `I2PSession.connect()`
  (blocks on real tunnels). No `HostLookup` resolution: the daemon
  answers `HostReply(Failure)` and go-i2cp only speaks HostLookup,
  so the §9 `bandwidth-or-destlookup` row is covered by the
  bandwidth alternative. No LeaseSet2 install for external
  sessions (fail-closed); negative-option/resource evidence beyond
  the lane rows stays local via the Plan 169 suites per §6.
- One `go-to-java-large` scheduling flake observed mid-session
  (data path fully digest-matched; only go's 15 s status-callback
  window missed). Absorbed by the §13 bounded retry; both closing
  runs passed attempt 1/3 on all rows.
- No public-I2P, remote-I2CP, tunnel, PQ, encrypted/meta-LS, or M6
  mixed-router claim. I2CP stays experimental, disabled by
  default, loopback-only.

## §8 pending (not executable from this session)

Working tree is uncommitted; on commit record the closing SHA here.
Routine CI and the manual `i2cp-external.yml` dispatch must go green
on the exact closing head before M9 is treated as closed in CI.
No Milestone 10 work is implemented in this plan.
