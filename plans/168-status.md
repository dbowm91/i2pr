# Plan 168 status — Milestone 9 I2CP message data plane

Status: **`passed-m9-i2cp-message-data-plane`**.

Registered: **2026-09-08**.

Plan of record:
[`plans/168-m9-i2cp-message-data-plane.md`](168-m9-i2cp-message-data-plane.md).

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

milestone8_final_acceptance = closed-via-plan161
milestone9_planning_authority = plan163
milestone9_protocol = i2cp
milestone9_wire_foundation = passed-via-plan164
milestone9_connection_session_options = passed-via-plan165
milestone9_client_owned_destination = passed-via-plan166
milestone9_i2cp_loopback_server_runtime = passed-via-plan167
milestone9_i2cp_message_data_plane = passed-via-plan168
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 169
next_product_layer = milestone9-i2cp
```

## What landed

### `i2pr-api::i2cp::data_plane` (new module)

The runtime-neutral Plan 168 surface lives in
[`crates/i2pr-api/src/i2cp/data_plane.rs`](../crates/i2pr-api/src/i2cp/data_plane.rs)
and is re-exported from `i2pr-api::i2cp`:

- `I2cpMessageOutcome` — bounded router-side outcome vocabulary
  (`Accepted`, `BadLocalLeaseSet`, `NoLocalTunnels`, `Overflow`,
  `DestinationStopping`, `BadSession`, `BadMessage`,
  `MessageExpired`, `BadExpirationHorizon`, `UnsupportedFlags`,
  `SessionError`); the outcome is the only source of truth for what
  the router promises in `MessageStatus` replies and the
  one-to-one `status_code()` mapping never overclaims end-to-end
  delivery. `Accepted` is the only success-class value in the
  bounded vocabulary.
- `I2cpDataPlaneAction::{EnqueueOutboundPayload, DeliverInboundPayload,
  ResolveDestinationLookup}` — the three typed data-plane actions
  the runtime emits. Every variant carries only typed values
  (session id, message id, nonce, protocol, ports, payload bytes);
  raw client bytes never appear in a payload.
- `PendingStatusTable` / `PendingStatusEntry` — bounded per-session
  correlation bookkeeping with explicit count ceiling
  (`MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION = 128`) and
  duplicate/late-idempotent `take()` semantics.
- `InboundPayloadQueue` / `InboundPayloadFrame` — bounded per-session
  inbound frame buffer with explicit frame
  (`MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION = 64`) and byte
  (`MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION = 64 KiB`) ceilings. The
  `InboundPayloadFrame::WIRE_OVERHEAD_BYTES = 14` constant is the
  only per-frame overhead the tests rely on.
- `DataPlaneError::CapacityExceeded` — single bounded-failure type
  shared by the correlation table and inbound queue.
- Bounded per-session ceilings
  (`MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION`,
  `MAX_CONCURRENT_DESTINATION_LOOKUPS_PER_CONNECTION`,
  `MAX_DESTINATION_LOOKUP_HORIZON = 10 s`,
  `MAX_MESSAGE_EXPIRATION_HORIZON = 1 h`) plus the helper accessors
  `max_pending_messages_per_session`,
  `max_pending_status_correlations`,
  `max_inbound_payload_bytes_per_session`,
  `max_destination_lookup_horizon`.

### `i2pr-daemon::i2cp` Plan 168 wiring

`crates/i2pr-daemon/src/i2cp.rs` extends the Plan 167 service
state with the Plan 168 data plane:

- `I2cpSessionState` is the per-session bookkeeping: bounded inbound
  queue, bounded pending-status table, outbound slot counter,
  router-side message-id counter, and a `tokio::sync::Notify`
  (`inbound_notify`) the connection task awaits between
  `read_chunk` branches.
- `SendMessage` / `SendMessageExpires` validation walks session
  ownership, payload gzip header, expiration horizon, and flag
  bits before any state mutates. The structural codec already
  rejects reserved flag bits 15-11; the data plane rejects the
  ElGamal-only low-tag-threshold / tags-to-send bits as
  `UnsupportedFlags` (mapped to `MessageStatus::BadOptions`).
- The router-side message id is allocated before enqueue; the
  correlation entry is removed when the destination runtime
  rejects the enqueue so duplicate/late internal events cannot
  resurrect a stale status.
- Outbound traffic routes through the existing
  `i2pr_client::DestinationRuntime::enqueue_outbound` seam; the
  runtime's `PayloadError` vocabulary maps one-to-one to the Plan
  168 outcomes (`Overflow`, `DestinationStopping`, `BadMessage`).
- Inbound `MessagePayload` delivery is sibling-isolated: the per-
  session queue accepts frames for the owning connection only, the
  connection task drains through the `inbound_notify`-driven
  `select!` branch.
- The cross-session local loopback shortcut routes payloads
  between two destinations owned by active I2CP sessions through
  the receiving session's inbound queue; non-loopback targets
  still report `Accepted` because the destination runtime accepts
  the payload locally.
- `DestLookup` resolves through the local destination registry;
  not-found returns the documented typed echo
  (`DestReplyBody::Hash`).
- `GetBandwidthLimits` returns the config-derived client ceiling
  (`max_buffered_bytes_per_connection / 1024`) and the documented
  neutral router values.

### Test surface

`crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` (new) holds
eighteen real-TCP black-box tests driven through raw
`tokio::net::TcpStream` bytes against the real listener on
`127.0.0.1:0`. Every Plan 168 §11 case is exercised:

- `small_payload_bidirectional_local_exchange` — two sessions
  owned by the same router exchange a small payload in each
  direction through the loopback shortcut, with the receiving
  connection observing the typed `MessagePayload` frame.
- `near_maximum_payload_is_accepted` — a payload at the
  `MAX_DESTINATION_PAYLOAD_BYTES = 32 KiB` boundary is accepted.
- `oversized_payload_rejected_before_routing` — payloads that
  exceed the destination ceiling fail through the runtime's
  `PayloadError` path.
- `send_message_expires_expired_in_past_returns_message_expired` /
  `send_message_expires_far_future_returns_bad_options` —
  expiration horizon enforcement.
- `send_message_expires_accepts_valid_future_expiration` /
  `send_message_expires_rejects_elgamal_tag_flags` — flag
  semantics, including the `NO_BUNDLE` informational bit.
- `dest_lookup_local_hit_returns_destination` /
  `dest_lookup_remote_not_found_returns_echoed_hash` — lookup
  resolution.
- `bandwidth_reply_uses_config_derived_client_ceiling` —
  bandwidth reply shape and source.
- `duplicate_message_id_is_idempotent` — pending-status
  correlation cleanup on connection exit.
- `send_before_session_active_is_rejected` — `BadSession`
  status for pre-session traffic.
- `malformed_gzip_metadata_returns_bad_message` — payload
  validation.
- `disconnect_restores_baseline_resources` /
  `disconnect_reason_does_not_block_data_plane_baselines` —
  teardown baselines.
- `unknown_destination_does_not_deliver_inbound` — loopback
  shortcut isolation.
- `gzip_constants_remain_stable` /
  `message_outcome_acceptance_is_honest` — pinned payload-format
  constants and the honest-status mapping.

The Plan 167 listener/runtime regression in
`crates/i2pr-daemon/tests/i2cp_loopback.rs` remains green (12
tests). The placeholder reference marker the file previously
carried has been pruned because the new file now exercises the
same types directly.

## Closure fields

Populated only from executed evidence:

```text
closing_sha = 379f1e3 (Plan 168 docs register)
implementation_sha = d328792 (Plan 168 implementation)
docs_sha   = 379f1e3 (single committed tree)
routine_ci_run = 34192916683 (Quality ubuntu-latest, all jobs passed)
routine_ci_ubuntu = passed
routine_ci_macos  = passed
msrv              = passed
dependency_policy = passed
```

Local floor on the closing tree (all executed 2026-09-08):

```text
cargo fmt --all --check                                                       = passed
cargo check --locked --workspace --all-targets                                = passed
cargo test --locked --workspace --all-targets -- --test-threads=1             = 1698 passed, 1 ignored (Plan 162 external SSU2 test)
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

Focused Plan 168 seams:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-api --test i2cp_vectors
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
```

All Plan 168 §13 acceptance criteria are satisfied:

1. `SendMessage`/`SendMessageExpires` validates session, payload,
   expiration, and flags before routing (the `enqueue_outbound_payload`
   helper enforces every check before any state mutates).
2. Outbound traffic reuses `i2pr_client::DestinationRuntime::enqueue_outbound`
   (no second routing stack; the daemon locks the destination
   registry, calls `get_mut`, and forwards the typed
   `DestinationPayload`).
3. I2CP payload encoding/decoding is bounded against expansion
   and malformed metadata — `MAX_DESTINATION_PAYLOAD_BYTES = 32 KiB`
   + `MAX_I2CP_PAYLOAD_BYTES = 64 KiB` + `MAX_I2CP_DECOMPRESSED_BYTES =
   64 KiB` ceilings are enforced through `Payload::new`, the
   destination runtime, and the `MAX_INBOUND_PAYLOAD_*` / outbound
   per-session ceilings.
4. Status mapping distinguishes local acceptance from stronger
   delivery (`I2cpMessageOutcome::Accepted` is the only success
   class in the bounded vocabulary; `status_code()` returns the
   matching `MessageStatusCode`).
5. Pending status / lookup / message state has count/byte/time
   ceilings and deterministic cleanup — every ceiling is a `const`
   in `data_plane.rs` and the `release_all` paths on session teardown
   zero every counter.
6. Authenticated inbound destination payload reaches only the
   owning I2CP session — the per-session inbound queue is keyed by
   the session id, and the connection task drains it through the
   `inbound_notify`-driven branch of its `select!`.
7. Slow clients cannot create unbounded output retention or harm a
   sibling session — the inbound queue enforces
   `MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION` and
   `MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION`; sibling-isolation is
   guaranteed because the queue is keyed by the session id.
8. Destination lookup uses existing local-registry seams and never
   invokes system DNS or address-book resolution — `handle_dest_lookup`
   walks the local destination table and the destination registry
   only; not-found returns the documented typed echo.
9. Bandwidth replies use real configuration/neutral spec values —
   `derive_bandwidth_reply` derives the client ceiling from
   `config.max_buffered_bytes_per_connection`; router-side limits
   stay at the documented neutral zero value.
10. Unsupported/draft flags are handled exactly per declared profile —
    the structural codec rejects reserved bits 15-11; the data
    plane rejects non-zero `tag_threshold` / `tags_to_send` bits as
    `UnsupportedFlags`; reliability-override bits 10-9 remain
    ignored per spec; Proposal 171 outbound-tunnel-switching bit
    remains documented but not implemented.
11. Bidirectional small and near-limit real-TCP local message
    exchange passes — `small_payload_bidirectional_local_exchange`
    and `near_maximum_payload_is_accepted` exercise the path.
12. M6 client and M7 SAM regressions remain green —
    `sam_stream_self_composed`, `sam_stream_final_acceptance`,
    `sam_stream_raw_product`, the Plan 167 listener regression in
    `i2cp_loopback.rs`, and the Plan 120–166 trajectories all
    pass.
13. Workspace floor and exact-head routine CI pass — see the
    local floor above.
14. No independent-client or public-network claim is made yet —
    the surface row in `specs/support.toml` and the CONFORMANCE
    entry explicitly retain `status = experimental`,
    `advertised = false`, and "no independent-client evidence;
    that belongs to Plans 169–170".
15. `plans/168-status.md` records exact local evidence and states
    Plan 169 is next — this file.

## Updates

- `crates/i2pr-api/src/i2cp/data_plane.rs` — new module with the
  typed data-plane vocabulary, bounded helpers, and helper
  constants.
- `crates/i2pr-api/src/i2cp/mod.rs` — registers the new module
  and re-exports the bounded constants/helpers.
- `crates/i2pr-api/src/i2cp/actions.rs` — clarifies that the
  data-plane actions live in `data_plane.rs` while keeping the
  `I2cpAction` enum the Plan 165 connection/session surface.
- `crates/i2pr-daemon/src/i2cp.rs` — adds the `I2cpSessionState`
  bookkeeping, `SendMessage` / `SendMessageExpires` handlers,
  `DestLookup` resolver, `GetBandwidthLimits` projection, and
  per-connection inbound draining via a `tokio::sync::Notify`.
- `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` — new
  eighteen-test black-box acceptance suite for every Plan 168
  §11 case.
- `crates/i2pr-daemon/tests/i2cp_loopback.rs` — prunes the
  placeholder reference marker (the new file exercises the same
  types directly).
- `specs/support.toml` — adds `plan_168_*` authority entries,
  bumps `milestone9_i2cp_message_data_plane = "passed-via-plan168"`,
  advances `next_executable_plan` to `"169"`, and adds the
  `i2cp.message-data-plane` surface row with the Plan 168
  evidence trail.
- `specs/CONFORMANCE.md` — extends the I2CP row with the Plan 168
  message/data-plane summary.
- `README.md` — adds the `plan_168 = passed-m9-i2cp-message-data-plane`
  classification row, advances `next_executable_plan` to `169`,
  and extends the Milestone 9 narrative with the Plan 168 surface
  summary.
- `AGENTS.md` — adds the Plan 168 current authority row, advances
  the M9 reading order to start at `plans/168-status.md`, adds
  Plan 168 to the focused-seams list and the protocol-claims
  list, and advances the handoff sentence.
- `plans/README.md` — adds the Plan 168 current authority block,
  the "What landed" / "What's not yet accepted" / "Current handoff"
  blocks, and the Plan 168 row in the Milestone 9 plan hierarchy.
- `.opencode/skills/i2pr-local-dev/SKILL.md` — Plan 168 added to the
  milestone status, "Retain these working pieces" list, protocol
  claims, and current handoff.
- `.opencode/skills/i2pr-architecture/SKILL.md` — Plan 168 added
  to the per-crate deep-dive index and the plan-of-record authority
  chain.
- `docs/architecture/i2pr-api.md` — new "Plan 168 — I2CP message
  data plane surface" section documenting the bounded helpers and
  the runtime-neutral data-plane vocabulary.
- `docs/architecture/i2pr-daemon.md` — new "Plan 168 — I2CP message
  data plane wiring" section documenting the `I2cpSessionState`
  bookkeeping and the data-plane dispatchers.
- `docs/architecture/i2pr-client.md` — new "Plan 168 — `enqueue_outbound`
  seam" section recording that the I2CP data plane is the documented
  caller of the existing `DestinationRuntime::enqueue_outbound`.
- `specs/protocols/10-i2cp-service-tunnels.md` — new "M9 I2CP
  message data plane (Plan 168)" section enumerating the bounded
  per-session ceilings, the honest status mapping, the inbound
  delivery rule, and the loopback shortcut.

## Handoff

Plan 168 is closed. Execute Plan **169**
([`plans/169-m9-i2cp-self-composed-local-product-and-hardening.md`](169-m9-i2cp-self-composed-local-product-and-hardening.md))
next, then Plan 170:

```text
plan_168 = passed-m9-i2cp-message-data-plane
next_executable_plan = 169
next_product_layer = milestone9-i2cp
```

Do not extend Plan 168's data plane into reconfiguration, the
self-composed local product hardening, or independent Java/Go
client evidence; those belong to the later M9 passes. SAM router-
owned product regressions remain green.
