# Plan 180 status — M10 service-tunnel composition, reconcile, and hardening

Status: **`passed-m10-service-tunnel-composition-reconcile-and-hardening`**.

Plan of record:
[`plans/180-m10-service-tunnel-composition-reconcile-and-hardening.md`](180-m10-service-tunnel-composition-reconcile-and-hardening.md).

Roadmap authority:
[`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](173-m10-service-tunnels-http-socks5-irc-roadmap.md)
([`plans/173-status.md`](173-status.md)).

Prior M10 authority:

- Foundation:
  [`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
  ([`plans/174-status.md`](174-status.md))
- Generic client/server tunnels:
  [`plans/175-m10-generic-client-server-service-tunnels.md`](175-m10-generic-client-server-service-tunnels.md)
  ([`plans/175-status.md`](175-status.md))
- HTTP `.i2p` proxy + CONNECT:
  [`plans/176-m10-http-i2p-proxy-and-connect.md`](176-m10-http-i2p-proxy-and-connect.md)
  ([`plans/176-status.md`](176-status.md))
- SOCKS5 `.i2p` CONNECT:
  [`plans/177-m10-socks5-i2p-connect-proxy.md`](177-m10-socks5-i2p-connect-proxy.md)
  ([`plans/177-status.md`](177-status.md))
- IRC `.i2p` client profile + privacy filter:
  [`plans/178-m10-irc-client-profile-and-privacy-filtering.md`](178-m10-irc-client-profile-and-privacy-filtering.md)
  ([`plans/178-status.md`](178-status.md))
- IRC `.i2p` server profile + authenticated peer hostname:
  [`plans/179-m10-irc-server-profile-and-authenticated-peer-hostname.md`](179-m10-irc-server-profile-and-authenticated-peer-hostname.md)
  ([`plans/179-status.md`](179-status.md))

## Current authority

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
plan_179 = passed-m10-irc-server-profile-and-authenticated-peer-hostname
plan_180 = passed-m10-service-tunnel-composition-reconcile-and-hardening
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = passed-via-plan178
milestone10_irc_server = passed-via-plan179
milestone10_local_product = passed-via-plan180
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 181
next_product_layer = milestone10-independent-acceptance
```

## What landed

```text
crates/i2pr-service-tunnels/src/generation.rs (new)
  Plan 180 §3/§5 runtime-neutral generation diff model:
  typed `DiffClass` (`Unchanged`, `MutableInPlace`,
  `ReplaceListener`, `ReplaceDestination`, `Remove`, `Add`),
  `ServiceDiff` (id + class + next spec), `diff_sets(committed,
  candidate)` and `diff_spec(prev, next)`; runtime-neutral,
  structural, deterministic, carries no secrets.

crates/i2pr-daemon/src/service_generation.rs (new)
  Plan 180 §3 committed-generation bookkeeping: `GenerationCounters`
  (active_current_generation, active_draining_generation,
  forced_drain_closes_total as `AtomicU64`), `ServiceTunnelGeneration`
  (generation_id, committed_specs `Arc<ServiceTunnelSet>`, runtimes,
  sam_destinations, destination_registry, counters),
  `DrainingGeneration` (generation, generation_id, drain_deadline,
  cancellation, destination_count), `GenerationIdAllocator` (atomic
  monotonic id source), `DestinationResolution` (typed cross-tunnel
  lookup result).

crates/i2pr-daemon/src/service_tunnels.rs (updated)
  Plan 180 §3/§4/§8/§9: `committed_generation`,
  `draining_generations`, and `generation_ids` fields on the
  manager. `reconcile(candidate, drain_deadline) -> ReconcileOutcome`
  validates the candidate, diffs against the committed generation,
  stages Add/Replace* services, publishes the new generation
  atomically, and pushes only replaced/removed old runtimes onto
  the draining list under a hard deadline. `committed_generation_id`,
  `draining_generation_count`, `reap_expired_drains -> ReapReport`,
  `generation_snapshot -> GenerationSnapshot`, and the unified
  cross-tunnel resource accounting matrix.
  `build_service_runtime` now returns a `StagedRuntime { runtime,
  destination_runtime }` pair so the reconcile commit path can
  consume the destination runtime into the per-generation
  destination registry (which is not `Clone`).

crates/i2pr-daemon/src/sam/streams.rs (updated)
  Plan 180 §3: `SamDestinations::install_handle(destination_id,
  handle)` accepts a pre-built `SamDestinationHandle` so the
  manager-level mirror and the per-generation directory can share
  the same bridge handle without rebuilding it.

crates/i2pr-daemon/src/service_tunnels_irc_server.rs (updated)
  Plan 180 §15: the test-only `ChannelInterceptionSource` switched
  from `tokio::sync::mpsc::unbounded_channel` to a bounded
  `tokio::sync::mpsc::channel(CHANNEL_INTERCEPTION_SOURCE_CAPACITY)`
  with capacity 64 so the production code path is bounded.

crates/i2pr-daemon/tests/service_tunnel_irc_server_product.rs (updated)
  All 21 IRC server product tests continue to pass after the test
  seam became bounded; `tx.send(...).expect(...)` calls became
  `tx.send(...).await.expect(...)`.

crates/i2pr-daemon/tests/service_tunnels_final_acceptance.rs (new)
  Plan 180 §11/§12: 15 black-box tests covering the Plan 180 §12
  matrix — no-op reconcile, add without disturbance, remove with
  sibling survival, target-change classification, bind-port-change
  classification, server-target-change identity preservation,
  invalid new alias fail-closed, candidate bind collision fail-closed,
  staged-destination-failure rollback, reap-expired-drains
  deadline handling, repeated-reconcile baseline stability, and
  forced-drain after deadline semantics.

crates/i2pr-daemon/tests/service_tunnels_adversarial_matrix.rs (new)
  Plan 180 §13: 12 black-box adversarial tests covering
  aggregate-flood split, shutdown during active reconcile,
  shutdown during drain, rapid repeated reconciles, drain deadline
  release, unknown-kind fail-closed, repeated invalid-target
  fail-closed, repeated no-op reconcile, post-shutdown reconcile,
  multiple reaps, client-kinds compile/run, and replace-bind
  classification.

scripts/check-service-tunnel-boundaries.sh (new)
  Plan 180 §15: static service-tunnel boundary checker enforcing
  the runtime-neutral constraint, the no-transport-internals
  constraint, no Garlic/I2NP construction in service-tunnels, no
  non-loopback listeners/targets in production daemon config, a
  single shared `run_stream_pump` definition (no duplicate raw
  byte pumps), no unbounded Tokio channels, and exactly one
  `register_service_tunnel_manager` entry point.

specs/support.toml (updated)
  `plan_180_plan_of_record`, `plan_180_status`,
  `milestone10_local_product = passed-via-plan180`,
  `next_executable_plan = 181`,
  `next_product_layer = milestone10-independent-acceptance`.

README.md, AGENTS.md, plans/README.md, .opencode/skills/i2pr-local-dev/SKILL.md,
docs/architecture/i2pr-service-tunnels.md,
specs/protocols/11-service-tunnels.md (normalized to local-product scope).
```

## Acceptance checklist (Plan 180 §18)

1. all enabled M10 services compose under one supervised manager
   — **passed** (`ServiceTunnelManager` owns all five kinds,
   `register_service_tunnel_manager` is the single entry point,
   `scripts/check-service-tunnel-boundaries.sh` enforces it).
2. candidate generations are fully staged before commit —
   **passed** (`reconcile` builds the new runtimes map and the
   per-generation destination registry from the staged map; the
   atomic swap only happens after the staged map is complete).
3. staging failure preserves the previous committed generation —
   **passed** (`service_tunnels_adversarial_matrix.rs`
   `unknown_kind_reconcile_fails_without_disturbing_committed`,
   `repeated_reconciles_with_invalid_target_fail_closed`).
4. unchanged service identities do not rotate —
   **passed** (`no_op_reconcile_retains_services_and_identity`,
   `add_service_via_reconcile_does_not_disturb_existing`,
   `change_server_target_keeps_identity_stable`).
5. old connections drain under a hard deadline while new
   admissions use only the committed generation —
   **passed** (`forced_drain_after_deadline_closes_only_old_generation`,
   `draining_generation_count_tracks_drain_lifecycle`,
   `reap_expired_drains` is idempotent).
6. aggregate ceilings cover every service kind and draining
   generation — **passed** (`GenerationSnapshot` exposes
   committed_active_connections, committed_draining_connections,
   committed_forced_drain_closes, draining_generations,
   draining_active_total, draining_forced_total`).
7. no-op/add/remove/target/bind/identity-failure reconcile matrix
   passes — **passed** (15 tests in
   `service_tunnels_final_acceptance.rs`).
8. integrated generic/HTTP/SOCKS/IRC local product moves
   application bytes only through public OS listener paths after
   startup — **passed** (the
   `service_tunnel_*_product.rs` suites retained; the manager
   owns the listener bind and the byte pump handoff uses the
   shared `run_stream_pump` only).
9. adversarial cross-service matrix returns every resource
   counter to baseline — **passed** (12 tests in
   `service_tunnels_adversarial_matrix.rs`).
10. static service-tunnel boundary checker is enforced in routine
    CI — **passed** (`scripts/check-service-tunnel-boundaries.sh`
    is in the routine CI floor).
11. SAM/M9 and earlier M10 focused regressions remain green —
    **passed** (full workspace test pass: 2115 passed, 1 ignored).
12. documentation/support claims are normalized to local-product
    scope — **passed** (this record).
13. full workspace floor and exact-head routine CI pass —
    **passed locally** (see Evidence).
14. `plans/180-status.md` records evidence and advances
    `next_executable_plan = 181` — **this record**.

## Evidence (Plan 180)

Generation/diff unit tests:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
# 222 passed (1 suite, 0.00s)
# Includes the Plan 174/175/176/177/178/179 runtime-neutral suite
# plus the new Plan 180 generation/diff module:
# identical_specs_classify_as_unchanged,
# resource_limit_change_is_mutable_in_place,
# listener_change_is_replace_destination,
# destination_change_is_replace_destination,
# server_target_change_is_replace_destination,
# kind_change_is_replace_destination,
# added_service_yields_add_diff,
# removed_service_yields_remove_diff,
# unchanged_spec_in_full_diff_is_classified_unchanged,
# is_no_op_recognizes_structural_noise.
```

Black-box Plan 180 §11/§12 reconcile matrix:

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_final_acceptance -- --test-threads=1
# 15 passed (1 suite, 1.30s)
```

Black-box Plan 180 §13 cross-service adversarial matrix:

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_adversarial_matrix -- --test-threads=1
# 12 passed (1 suite, 0.44s)
```

Plan 174/175/176/177/178/179 regressions:

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation -- --test-threads=1
# 7 passed (1 suite, 0.00s)
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
# 9 passed (1 suite, 0.12s)
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
# 15 passed (1 suite, 35.05s)
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
# 23 passed (1 suite, 0.09s)
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
# 18 passed (1 suite, 37.08s)
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_server_product -- --test-threads=1
# 21 passed (1 suite, 0.15s)
```

Full workspace:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
# 2115 passed, 1 ignored (78 suites, 274.73s)
```

Static gates:

```text
cargo fmt --all -- --check
# ok
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# ok
cargo test --locked --workspace --doc
# 0 passed (16 suites, 0.00s)
bash scripts/check-dependency-direction.sh
# dependency direction: ok
bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed
bash scripts/check-service-tunnel-boundaries.sh
# service-tunnel boundary checks passed
bash scripts/check-fixture-manifest.sh
# ok
bash scripts/check-i2cp-vectors.sh
# I2CP vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh
# SSU2 vector manifest is complete and hashes match.
bash scripts/check-ntcp2-vectors.sh
# NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh
# Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh
# Plan 077 constrained-host lane boundary checks passed
bash scripts/check-sam-acceptance-evidence.sh
# SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh
# SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh
# I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# 153 tests, OK
```

## Stop conditions (Plan 180 §19)

None of the Plan 180 §19 stop conditions fired:

- atomic generation replacement does NOT require a global mutable
  router context (the manager owns its committed/draining state).
- listener replacement preserves old connections via Arc-shared
  bridge handles (no `unsafe`, no `SO_REUSEPORT` dependency).
- stable identities rotate only on ReplaceDestination, never on
  Unchanged/MutableInPlace.
- aggregate resource accounting bounds every service kind via the
  shared `aggregate_permit` semaphore and the per-generation
  `GenerationCounters`.
- the canonical Plan 180 final-acceptance product uses only public
  OS listener paths after startup (the
  `service_tunnel_*_product.rs` suites all bind via real TCP).
- no M6 Streaming/destination correctness defect was exposed.

## Handoff

Execute Plan **181** next (independent M10 acceptance / final
closure). Do not begin Plan 181 without an explicit passing
status record for this plan.

```text
plan_180 = passed-m10-service-tunnel-composition-reconcile-and-hardening
milestone10_local_product = passed-via-plan180
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 181
```
