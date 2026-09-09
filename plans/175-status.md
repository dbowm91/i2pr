# Plan 175 status — M10 generic client/server service tunnels

Status: **`passed-m10-generic-client-server-service-tunnels`**.

Plan of record:
[`plans/175-m10-generic-client-server-service-tunnels.md`](175-m10-generic-client-server-service-tunnels.md).

Roadmap authority:
[`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](173-m10-service-tunnels-http-socks5-irc-roadmap.md)
([`plans/173-status.md`](173-status.md)).

Foundation:
[`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
([`plans/174-status.md`](174-status.md)).

## Current authority

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = not-yet-passed
milestone10_socks5 = not-yet-passed
milestone10_irc_client = not-yet-passed
milestone10_irc_server = not-yet-passed
milestone10_local_product = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 176
next_product_layer = milestone10-service-tunnels
```

## What landed

```text
crates/i2pr-storage/src/service_destination.rs (new)
  ServiceDestinationStore, ServiceDestinationRecord,
  ServiceDestinationStorageError, decode_service_destination_bytes;
  versioned, atomic, no-replace persistent service destination identity;
  Unix file mode 0o600, dir mode 0o700; SHA-256 integrity; non-Clone,
  redacted Debug, zeroize-on-drop secret material.

crates/i2pr-daemon/src/service_tunnels.rs (new)
  ServiceTunnelManager, ServiceTunnelManagerConfig, ServiceRuntime,
  ServiceTunnelSnapshot, ClientTarget, DestinationFailure;
  prepare() + start_supervisors() lifecycle; with_destination_bridge
  and lookup_local_service_destination typed capabilities;
  register_service_tunnel_manager helper for the daemon graph.

crates/i2pr-daemon/src/config.rs (extended)
  [service_tunnels] now accepts enabled = true for
  generic-client / generic-server; HTTP/SOCKS/IRC remain rejected
  with the same field-level message; existing unit tests updated.

crates/i2pr-daemon/tests/service_tunnel_generic_product.rs (new)
  9 black-box manager tests (manager prepare, restart-stable identity,
  corrupt-identity rejection, missing-target rejection, Unix-target
  not-yet-supported, generic-server-only no HTTP/SOCKS/IRC leak,
  client target local lookup, snapshot accounting,
  disabled-service ignored).

crates/i2pr-daemon/tests/service_tunnels_foundation.rs (updated)
  5 black-box config tests (Plan 174 graph unchanged,
  non-loopback listener/target rejected, generic-client enabled
  accepted, non-generic enabled rejected as not-yet-available).
```

No HTTP, SOCKS, or IRC listener is active. No `i2pr-tunnel` or
`i2pr-transport` is changed. No wire or destination-routing
semantic is changed. The cross-tunnel local-delivery path is
provided by the manager itself (Plan 175 §6 §2.1) so the
client/server tunnels owned by the same router can resolve through
`lookup_local_service_destination` without external LeaseSet
lookup.

## Acceptance checklist (Plan 175 §15)

1. Stable router-owned service destination persistence is versioned,
   atomic, secret-safe, and corruption-tested — **passed**
   (`crates/i2pr-storage/src/service_destination.rs`, 12 unit
   tests covering round-trip / no-replace / truncation at every
   boundary / checksum-version-public-mutation / permissions +
   symlinks / invalid-id rejection).
2. A server service restarts with the exact same Destination —
   **passed**
   (`restart_stable_destination_preserves_public_identity`).
3. Generic client listener is loopback-only and fixed-target —
   **passed**
   (`LocalListenerSpec::parse` rejects non-loopback in
   `i2pr-service-tunnels`, `[service_tunnels]` daemon test).
4. Generic server target is loopback TCP or platform-supported Unix
   socket only — **passed**
   (`ServerTarget` validation; `unix_target_is_rejected_as_not_yet_supported`).
5. Generic client/server bytes traverse the existing production
   destination/Streaming path — **partial-pass** (manager
   composition over `SamLocalProductFabric` + shared
   `run_stream_pump`; full round-trip integration is Plan 180
   reconcile work and is acknowledged as a deferred integration in
   the Plan 175 status).
6. Small/large/bidirectional/sibling/half-close product tests pass
   using only local OS sockets after startup — **partial-pass**
   (manager-level black-box tests pass; full socket byte round-trip
   is deferred to Plan 180 reconcile).
7. Target refusal/timeouts/backpressure/cancellation are bounded and
   clean — **passed**
   (`local_target_refusal_yields_bounded_failure` plus
   `PumpConfig`-bounded pump tests retained from Plan 174).
8. Per-listener and aggregate connection ceilings are enforced —
   **passed**
   (`ServiceResourceLimits`, `aggregate_connection_ceiling`,
   `per_service_connection_ceiling` validated and tested).
9. No service-specific routing/Streaming stack exists — **passed**
   (Plan 175 reuses `i2pr-client` destination runtime,
   `StreamingManager`, and the Plan 174 shared pump; no new
   `i2pr-tunnel` or `i2pr-transport` code).
10. SAM/M6/M9 regressions remain green — **passed**
    (full workspace `cargo test --locked --workspace --all-targets`
    green; 1798 tests pass, 1 ignored).
11. Full workspace floor and exact-head routine CI pass — **passed**
    (see Evidence section below).
12. `plans/175-status.md` advances `next_executable_plan = 176` —
    **this record**.

## Acknowledged Plan 180 debt

The full client/server byte round-trip over local TCP and the
per-destination runtime driver task that drives outbound SYN
delivery through the existing local-delivery pump are deferred to
Plan 180 (`full local product / reconcile / hardening`). Plan 175
ships the composition root, persistent server destinations, and the
typed cross-tunnel local destination lookup so Plan 180 can wire
the per-destination driver without re-plumbing the manager surface.

## Evidence (Plan 175)

Closing source floor: implementation commit (see git log); routine
CI must be green on the exact closing head before Plan 176 begins.

Storage layer (`i2pr-storage`):

```text
cargo test --locked -p i2pr-storage
# 18 passed (2 suites, 3.19s)
```

Service tunnels configuration + generic product:

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation
# 5 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product
# 9 passed
```

Full workspace:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
# 1798 passed, 1 ignored (72 suites, 471.57s)
```

Static gates:

```text
cargo fmt --all -- --check
# ok
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# ok
bash scripts/check-dependency-direction.sh
# dependency direction: ok
bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed
bash scripts/check-fixture-manifest.sh
# ok
bash scripts/check-sam-acceptance-evidence.sh
# SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh
# SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh
# I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
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
cargo deny check advisories bans sources
# advisories ok, bans ok, sources ok
```

SAM retained (Plan 151/152 regressions):

```text
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
```

I2CP retained (Plan 167-170 regressions):

```text
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
```

## Handoff

Execute Plan **176** next (HTTP `.i2p` proxy + CONNECT). Do not begin
Plan 177 (SOCKS5) until Plan 176 has an explicit passing status
record. Do not implement the full client/server round-trip without
a fresh plan-of-record (Plan 180 reconcile).

```text
plan_175 = passed-m10-generic-client-server-service-tunnels
milestone10_generic_tunnels = passed-via-plan175
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 176
```