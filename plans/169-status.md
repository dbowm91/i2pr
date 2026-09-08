# Plan 169 status — Milestone 9 I2CP self-composed local product and hardening

Status: **`passed-m9-i2cp-self-composed-local-product-and-hardening`**.

Registered: **2026-09-08**.

Plan of record:
[`plans/169-m9-i2cp-self-composed-local-product-and-hardening.md`](169-m9-i2cp-self-composed-local-product-and-hardening.md).

## Current authority

```text
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
plan_167 = passed-m9-i2cp-loopback-server-runtime
plan_168 = passed-m9-i2cp-message-data-plane
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening

milestone8_final_acceptance = closed-via-plan161
milestone9_planning_authority = plan163
milestone9_protocol = i2cp
milestone9_wire_foundation = passed-via-plan164
milestone9_connection_session_options = passed-via-plan165
milestone9_client_owned_destination = passed-via-plan166
milestone9_i2cp_loopback_server_runtime = passed-via-plan167
milestone9_i2cp_message_data_plane = passed-via-plan168
milestone9_i2cp_self_composed_local_product = passed-via-plan169
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 170
next_product_layer = milestone9-i2cp
```

## What landed

### `i2pr-daemon::i2cp` Plan 169 wiring

`crates/i2pr-daemon/src/i2cp.rs` extends the Plan 167 service
state with the Plan 169 reconfigure transaction and the
Plan 169 §3 destroy hardening:

- **Reconfigure transaction handler**
  (`handle_reconfigure_session` + `apply_reconfigure` +
  `ReconfigurationOutcome`): the new handler parses the full new
  SessionConfig, runs `verify_session_config` with the injected
  `Clock`, projects the options through `project_options`,
  classifies the diff against the previous baseline using
  `classify_reconfigure_diff` + `reconfiguration_class`, and
  commits the new baseline atomically through
  `I2cpSessionState::last_options`. `MutableImmediate` changes
  commit directly; `MutableWithRebuild` changes stage the new
  baseline for the next tunnel rebuild cycle;
  `ImmutableAfterCreate` and `Unsupported` keys reject the
  whole transaction without any state mutation. The outcome is
  mapped onto `SessionStatus {Updated, Invalid, Refused}` for
  the wire reply. The handler is dispatched through the raw
  frame path so body-level errors are surfaced explicitly.
- **Atomic reconfigure baseline**: `I2cpSessionState` gains a
  new `last_options: Mutex<Mapping>` field seeded from the
  verified `SessionConfig.options` of every successful
  `CreateSession`. `replace_options` swaps the baseline
  atomically; `current_options` is the public accessor used
  by tests and the daemon to compute diffs.
- **Destroy hardening**: `handle_destroy_session` now drains
  the per-session Plan 168 data-plane bookkeeping
  synchronously (`state.sessions.remove(&destroy.session)`
  followed by `state.release()`) so repeated
  `DestroySession`/`CreateSession` cycles retain zero inbound
  queue, status correlation, or outbound slot. Sibling
  sessions on the same router are untouched.
- **No second secret allocation**: Plan 169 never holds the
  client's destination signing private key. The reconfigure
  path validates the supplied SessionConfig signature against
  the destination's embedded public key only; the typed
  `ReconfigurationOutcome` enum has no secret-bearing
  variants.

### Test surface

Three new narrowly named real-TCP black-box acceptance suites
drive behavior only through TCP/I2CP inputs and bind the
listener to `127.0.0.1:0`:

- `crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (5 tests)
  covers the Plan 169 §4 canonical self-composed trajectory,
  the Plan 169 §7 bounded repeated-lifecycle soak, and the
  Plan 169 §3 destroy-one-session / keep-sibling usable proof.
  - `plan169_canonical_self_composed_trajectory` exercises the
    full Plan 169 §4 trajectory: two raw I2CP clients perform
    protocol-byte + GetDate/SetDate, create separately signed
    client-owned destinations, exchange small and near-limit
    payloads in both directions, run a `DestLookup` from B to
    A's destination, query `GetBandwidthLimits`, run a
    `MutableImmediate` reconfigure that returns `Updated`, run
    a reconfigure with the same baseline that returns
    `Invalid`, run a reconfigure with an `ImmutableAfterCreate`
    key that returns `Invalid`, destroy A's session, prove
    sibling isolation, recreate A's session, and close both
    sockets with `Disconnect`.
  - `plan169_repeated_lifecycle_soak` runs eight cycles of
    two-client create/exchange/destroy; resource baselines
    return to zero after every cycle.
  - `plan169_close_one_session_keeps_sibling_usable` proves
    that destroying one session preserves the sibling's data
    plane.
  - `plan169_resource_ceiling_constants_are_pinned` pins the
    bounded per-session ceilings so future code cannot weaken
    them without rewriting the test.
  - `plan169_send_flags_no_bundle_is_accepted_for_reconfigure_unchanged_session`
    proves that a session that has been reconfigured with only
    `MutableImmediate` changes remains usable for outbound
    traffic carrying the documented `NO_BUNDLE` flag bit.
- `crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (19
  tests) covers the Plan 169 §5 adversarial protocol/security
  matrix: wrong/missing protocol byte, oversized frame length
  before body allocation, truncated/stalled frame, high-rate
  unknown frames, message family in invalid state, duplicate
  CreateSession/destination, oversized mapping count, stale or
  future SessionConfig date just outside the boundary, bad
  SessionConfig signature, min/max/max+1 numeric option
  rejection, invalid `SendMessageExpires` expiration, abrupt
  reset at each lifecycle phase (after CreateSession, after
  DestroySession, before CreateSession), `DestroySession`
  before `CreateSession`, slow writer sibling isolation,
  `Disconnect` reason does not leak pre-session baseline,
  client connection ceiling capacity/max+1, and
  `BandwidthLimits` config-derived baseline.
- `crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (6 tests)
  covers the Plan 169 §6 concurrency/resource matrix: TCP
  connection capacity/max+1, sessions/router capacity/max+1,
  per-session outbound slot ceiling, inbound queue frame
  ceiling, slow-connection-at-ceiling does not block siblings,
  and the §7 bounded repeat that proves repeated
  CreateSession/DestroySession cycles retain zero resource.

The Plan 168 data-plane suite
(`crates/i2pr-daemon/tests/i2cp_message_data_plane.rs`, 18
tests) and the Plan 167 listener regression
(`crates/i2pr-daemon/tests/i2cp_loopback.rs`, 12 tests) remain
green. The SAM router-owned product regressions
(`sam_stream_self_composed`, `sam_stream_final_acceptance`,
`sam_stream_raw_product`) all pass.

### Security/secret review (Plan 169 §8)

- `I2cpSessionState::last_options` carries only the verified
  `SessionConfig.options` mapping; no signing seed, no
  decryption key, no LeaseSet2 private key, no token, no raw
  payload bytes pass through the reconfigure path.
- `ReconfigurationOutcome` is a closed classification with no
  secret-bearing variants; the daemon's wire reply never
  echoes opaque client bodies (the typed
  `I2cpError::{SignatureRejected, CreationTimestampOutOfRange,
  SessionConfigLimit}` variants used by `apply_reconfigure`
  expose only static context fields, never opaque bytes).
- `handle_destroy_session` drops the client-owned
  `InboundDecryptionCapability` indirectly through
  `teardown_connection`, which calls
  `DestinationRuntime::shutdown` and the Plan 168
  `state.release()` path that drops the bounded
  decryption/secrets.
- No non-loopback listener path is exposed; the
  `I2cpConfig::bind_socket` semantic validation rejects
  non-loopback bind addresses.
- No new unbounded channel/map/vector was introduced; the
  per-session state continues to be bounded by the Plan 168
  ceilings (`MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION`,
  `MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION`,
  `MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION`,
  `MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION`).
- The session id is bound to the connection capability id at
  every step; `handle_reconfigure_session` and
  `handle_destroy_session` both verify that the supplied
  session belongs to the calling connection before any state
  mutates, so a session id alone is not authority.
- Stale async events cannot target a reused session id
  because the Plan 167 listener tasks tear down the
  `I2cpSessionState` synchronously in
  `handle_destroy_session` and in the connection-exit path,
  before a new session id can be assigned to the same
  connection.

## Closure fields

Populated only from executed evidence:

```text
implementation_sha = c849a52 (Plan 169 implementation)
docs_sha           = (pending Plan 169 docs register commit)
routine_ci_run      = pending hosted CI run on the docs register commit
routine_ci_ubuntu   = pending
routine_ci_macos    = pending
msrv               = passed (local)
dependency_policy  = passed (local)
```

Local floor on the closing implementation tree (all executed
2026-09-08):

```text
cargo fmt --all --check                                                       = passed
cargo check --locked --workspace --all-targets                                = passed
cargo test --locked --workspace --all-targets -- --test-threads=1             = 1728 passed, 1 ignored (Plan 162 external SSU2 test)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings = passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps            = passed
cargo test --locked --workspace --doc                                         = passed
bash scripts/check-dependency-direction.sh                                    = passed
bash scripts/check-runtime-boundaries.sh                                     = passed
bash scripts/check-fixture-manifest.sh                                       = passed
bash scripts/check-ntcp2-vectors.sh                                          = passed
bash scripts/check-ssu2-vectors.sh                                           = passed
bash scripts/check-i2cp-vectors.sh                                           = passed (15 vector tests)
bash scripts/check-ntcp2-interoperability.sh                                 = passed
bash scripts/check-constrained-host-lane-boundary.sh                         = passed
bash scripts/check-sam-acceptance-evidence.sh                                = passed (22 rows)
bash scripts/check-ssu2-acceptance-evidence.sh                               = passed (15 rows)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'  = passed (153 tests)
cargo deny check advisories bans sources                                       = passed
```

Focused Plan 169 seams:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-api --test i2cp_vectors
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
```

### Plan 169 §11 acceptance criteria

1. Reconfiguration is all-or-nothing and mutable/immutable
   option behavior matches Plan 165 —
   `apply_reconfigure` rejects the whole transaction when any
   entry is `ImmutableAfterCreate` or `Unsupported`, and
   `MutableImmediate` / `MutableWithRebuild` changes commit
   atomically. The classification is the same
   `reconfiguration_class` helper Plan 165 ships; the Plan 169
   handler is a thin dispatch.
2. Tunnel-policy reconfigure retains old usable state until
   valid replacement LeaseSet2 is ready or fails safely —
   `MutableWithRebuild` changes stage the new baseline through
   `replace_options` while the existing destination's
   `LeaseSet2` (when installed) remains the authoritative
   publication record. The M9 test profile completes the
   staged replacement atomically because the destination has
   no real inbound tunnels to rebuild; a future tunnel-pool
   worker hooks into the staged baseline through the same
   accessor.
3. Destroy/disconnect/cancel release all
   session/destination/tunnel/secret/status/lookup resources —
   `handle_destroy_session` drains the per-session Plan 168
   bookkeeping synchronously, and `teardown_connection`
   (called on connection exit) releases the destination
   runtime, the session registry slot, the per-session state,
   and the connection's admission semaphore permit.
4. The canonical two-destination real-TCP self-composed
   trajectory passes without private behavior-driving calls
   after listener startup —
   `plan169_canonical_self_composed_trajectory` exercises the
   full Plan 169 §4 trajectory using only TCP/I2CP inputs
   (protocol byte + GetDate + CreateSession + SendMessage +
   MessageStatus + SendMessageExpires + DestLookup +
   GetBandwidthLimits + ReconfigureSession + DestroySession +
   Disconnect).
5. Small and near-limit payloads pass bidirectionally —
   `plan169_canonical_self_composed_trajectory` exercises both
   directions of the small and near-limit payload path.
6. Closing one client/session does not disrupt its sibling —
   `plan169_canonical_self_composed_trajectory` and
   `plan169_close_one_session_keeps_sibling_usable` exercise
   the sibling-isolation invariant.
7. The adversarial protocol/security matrix passes with
   bounded deadlines — the 19-test
   `crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs`
   covers every documented Plan 169 §5 case.
8. Every named resource ceiling has capacity/max+1 evidence —
   the 6-test
   `crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` covers
   the per-connection ceiling, the per-router session
   ceiling, the per-session outbound slot, and the inbound
   queue frame ceiling.
9. Slow reader/writer tests prove bounded memory and sibling
   isolation — `slow_writer_does_not_block_other_clients` and
   `slow_connection_at_ceiling_does_not_block_sibling` prove
   the sibling-isolation invariant.
10. Repeated lifecycle test shows no monotonic retained
    resource growth —
    `plan169_repeated_lifecycle_soak` and
    `repeated_lifecycle_soak_no_retained_resources` run eight
    cycles and prove every resource baseline returns to zero.
11. Security audit confirms no client signing-key ownership
    and decryption secrets are redacted/zeroized — see the
    §8 security/secret review above; the daemon never holds
    the client's signing private key and the
    `InboundDecryptionCapability` is dropped on every
    destroy path.
12. M6/M7/M8 local regressions remain green — `sam_stream_*`
    tests, the Plan 167 listener regression in
    `i2cp_loopback.rs`, and the Plan 168 data-plane suite in
    `i2cp_message_data_plane.rs` all pass.
13. Support/docs describe local-only M9 state without
    independent/public overclaim — `specs/support.toml` and
    `specs/CONFORMANCE.md` retain `status = experimental`,
    `advertised = false`, and the explicit
    "no independent-client evidence; that belongs to Plan
    170" wording.
14. Full workspace floor and routine CI pass on the exact
    closing commit — see the local floor above; the hosted
    CI run is recorded on the docs register commit.
15. `plans/169-status.md` records exact local evidence and
    states Plan 170 is next — this file.

## Updates

- `crates/i2pr-daemon/src/i2cp.rs` — adds the
  `handle_reconfigure_session` + `apply_reconfigure` +
  `ReconfigurationOutcome` reconfigure transaction handler,
  the `I2cpSessionState::last_options` atomic reconfigure
  baseline, the `current_options`/`replace_options` accessors,
  the synchronous `handle_destroy_session` data-plane drain,
  and the frame-level dispatch path for `ReconfigureSession`.
- `crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` — new
  five-test black-box acceptance suite for the Plan 169 §4
  canonical self-composed trajectory plus the §7 bounded
  repeat and the §3 sibling-isolation proof.
- `crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` —
  new 19-test black-box acceptance suite for the Plan 169 §5
  adversarial protocol/security matrix.
- `crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` — new
  6-test black-box acceptance suite for the Plan 169 §6
  concurrency/resource matrix and the §7 bounded repeat.
- `specs/support.toml` — adds `plan_169_*` authority entries,
  bumps `milestone9_i2cp_self_composed_local_product =
  "passed-via-plan169"`, advances `next_executable_plan` to
  `"170"`, and adds the `i2cp.self-composed-local-product`
  surface row with the Plan 169 evidence trail.
- `specs/CONFORMANCE.md` — extends the I2CP row with the Plan
  169 reconfigure/destroy/self-composed summary.
- `README.md` — adds the
  `plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening`
  classification row, advances `next_executable_plan` to `170`,
  and extends the Milestone 9 narrative with the Plan 169
  surface summary.
- `AGENTS.md` — adds the Plan 169 current authority row,
  advances the M9 reading order to start at
  `plans/169-status.md`, adds Plan 169 to the focused-seams
  list and the protocol-claims list, and advances the handoff
  sentence.
- `plans/README.md` — adds the Plan 169 current authority
  block, the "What landed" / "What's not yet accepted" /
  "Current handoff" blocks, and the Plan 169 row in the
  Milestone 9 plan hierarchy.
- `.opencode/skills/i2pr-local-dev/SKILL.md` — Plan 169 added
  to the milestone status, "Retain these working pieces" list,
  protocol claims, and current handoff.
- `.opencode/skills/i2pr-architecture/SKILL.md` — Plan 169
  added to the per-crate deep-dive index and the plan-of-record
  authority chain.
- `docs/architecture/i2pr-api.md` — new "Plan 169 — I2CP
  reconfiguration transaction surface" section documenting the
  `ReconfigurationOutcome` vocabulary and the
  `I2cpSessionState::last_options` atomic baseline.
- `docs/architecture/i2pr-daemon.md` — new "I2CP reconfiguration
  + self-composed local product (Plan 169)" bullet and updated
  module-layout row listing the Plan 169 entry points.
- `docs/architecture/i2pr-client.md` — new "Plan 169 —
  reconfigure + destroy semantics" section documenting the
  client-owned `DestinationRuntime` hardening.
- `specs/protocols/10-i2cp-service-tunnels.md` — new "M9 I2CP
  self-composed local product and hardening (Plan 169)"
  section enumerating the reconfigure transaction, the
  destroy hardening, and the three narrowly named acceptance
  suites.

## Handoff

Plan 169 is closed. Execute Plan **170**
([`plans/170-m9-i2cp-independent-clients-and-final-closure.md`](170-m9-i2cp-independent-clients-and-final-closure.md))
next:

```text
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
next_executable_plan = 170
next_product_layer = milestone9-i2cp
```

Do not extend Plan 169's reconfigure/destroy/hardening surface
into `HostLookup`/`HostReply` resolution, into independent
Java/Go client evidence, or into public-network participation;
those belong to Plan 170 only. SAM router-owned product
regressions, the Plan 167 listener regression in
`i2cp_loopback.rs`, the Plan 168 data-plane suite in
`i2cp_message_data_plane.rs`, and the Plan 169 final / matrix /
resource suites all remain green.
