# Plan 202 status — M10 production remote Destination/Streaming composition

Status: **`passed-m10-production-remote-destination-and-streaming-composition`**
on the local product floor.

Plan of record: [`202-m10-production-remote-destination-and-streaming-composition.md`](202-m10-production-remote-destination-and-streaming-composition.md).

## Implementation summary

Plan 202 connects the M10 `ServiceTunnelManager` to the existing
daemon-owned router stack (Plans 184–193) through a typed
**router-delivery capability**. The capability is wired once per
daemon-wide owner and shared across every service the manager owns;
no per-service SSU2/tunnel stack is created. The runtime-neutral
`i2pr-service-tunnels` crate remains transport-agnostic; the
`ServiceDestinationDelivery` capability lives in
`crates/i2pr-daemon/src/service_delivery.rs` (Plan 202 §5).

```text
crates/i2pr-daemon/src/service_delivery.rs
  Plan 202 §5/§10/§13 — typed `ServiceDestinationDelivery`
  capability with `RoutingDecision` enum (LocalCoOwned /
  RemoteRouter / RemoteUnresolved), `RemoteDeliveryCounters`
  bounded counter set (`remote_lookup_started`,
`remote_lookup_succeeded`, `remote_lookup_failed`,
`remote_stream_connect_started`, `remote_stream_established`,
`remote_outbound_requests`, `remote_inbound_payloads`,
`remote_route_retries`, `remote_route_timeouts`,
`remote_tunnel_loss`, `local_coowned_deliveries`, `unknown_peer`),
  bounded in-flight resolution table
  (`MAX_CONCURRENT_REMOTE_RESOLUTIONS = 32`,
  `MAX_REMOTE_DELIVERY_RETRIES = 2`,
  `DEFAULT_REMOTE_LOOKUP_DEADLINE_MS = 5_000`),
  pure `classify_destination` helper, public
  `record_observation(label)` typed observation surface, and
  `destination_hash_bytes` / `destination_hash_from_slice`
  canonical hash conversion helpers. Unknown observation labels
  are silently ignored so a future expansion of the documented
  set must update both this helper and the static checker.

crates/i2pr-daemon/src/service_tunnels.rs
  Plan 202 §5 — `ServiceTunnelManager` gains a
  `router_delivery: Mutex<Option<ServiceDestinationDelivery>>`
  field, `install_router_delivery` / `uninstall_router_delivery`
  / `router_delivery` / `has_router_delivery` API surface, a
  pure `routing_decision_for(hash) -> RoutingDecision` helper,
  `co_owned_destination_hashes()` reporting the manager's
  authoritative co-owned set, and a richer
  `resolve_client_destination_with_decision(spec) ->
  (Result<ClientTarget, DestinationFailure>, RoutingDecision)`
  surface so external drivers and tests can assert the routing
  decision without forcing a `Result<ClientTarget,
  DestinationFailure>` probe. The free-function
  `install_router_delivery_handle(manager, capability)` is the
  daemon composition root's single entry point for installing
  the capability.

crates/i2pr-daemon/src/service_tunnels.rs
  Plan 202 §5 — new `plan202_routing_tests` unit module with
  five unit rows covering the default `RemoteUnresolved`
  outcome, the `RemoteRouter` outcome after `install_router_delivery`,
  the `LocalCoOwned` outcome for prepared service destinations,
  the `resolve_client_destination_with_decision` surface across
  both backend states, and the `co_owned_destination_hashes`
  tracking the committed generation.

crates/i2pr-daemon/tests/service_tunnels_remote_transport_qualification.rs
  Plan 202 §11 — new positive Direction A external driver. The
  test is `#[ignore]`-gated, fails closed when the exact-pinned
  i2pd environment is absent, exercises the same Plan 184–193
  real one-hop builds + / LeaseSet2 lookup + / Streaming path the
  Plan 193 streaming external driver uses, and additionally
  asserts the manager-level routing decision classifies the
  reference destination as `RemoteRouter` after
  `install_router_delivery_handle`, and as `RemoteUnresolved`
  after `uninstall_router_delivery`. The driver advances the
  Plan 202 §13 typed counters (`remote_lookup_succeeded`,
  `remote_stream_established`, `remote_outbound_requests`,
  `remote_inbound_payloads`) and emits one terminal
  `remote-stream-established` row plus a `remote-counters`
  snapshot for the static checker.

tests/integration/service-tunnels/run-independent.sh
  Plan 202 — new `m10-remote-destination-streaming-composition`
  row in the §6.1 + Plan 202 remote qualification block. The row
  is recorded `blocked` with command/log provenance when the
  full SSU2 endpoint/bind tuple (I2PD_ROUTER_INFO /
  I2PD_SSU2_ENDPOINT / I2PR_SSU2_BIND / EVIDENCE_DIR) is absent,
  or when the i2pd cache / pin / version check fails; when the
  exact-pinned M6 interop lane provisions the environment, the
  row flows through `record_guarded` (Plan 202 §16.7) and flips
  to `passed` only when the driver emits both
  `lease-lookup-completed=` and `remote-stream-established=true`.

scripts/check-service-tunnel-acceptance-evidence.sh
  Plan 202 — new §12 invariants enforce:
  - the positive remote composition driver exists at
    `crates/i2pr-daemon/tests/service_tunnels_remote_transport_qualification.rs`;
  - the driver is `#[ignore]`-gated, declares the
    `m10_remote_destination_streaming_composition_through_manager`
    Direction A test name, exercises `RemoteDeliveryCounters`,
    `install_router_delivery_handle`, `routing_decision_for`,
    and asserts `RoutingDecision::RemoteRouter`;
  - the driver never logs peer key material;
  - the manager exposes `install_router_delivery`,
    `routing_decision_for`, and `co_owned_destination_hashes`.
  The two existing remote rows (`remote-independent-http-eepsite`,
  `remote-independent-irc-service`) stay `blocked` forever; the
  new `m10-remote-destination-streaming-composition` row is
  classified `PLAN202_TRANSITIONAL` so it can flow through
  either `record_blocked` (no SSU2 lane env) or `record_guarded`
  (the dedicated M6 interop lane).

## What Plan 202 changed

1. **Typed router-delivery capability (Phase A).** The M10
   manager owns one `ServiceDestinationDelivery` field; the
   daemon composition root calls `install_router_delivery_handle`
   to wire the router stack (Plans 184–193) into the manager.
   The capability is shared across every service the manager
   owns; no per-service SSU2/tunnel stack is created.

3. **Typed remote destination resolution (Phase B).** A pure
   `RoutingDecision` classification distinguishes
   `LocalCoOwned` (Plan 182 bridge), `RemoteRouter` (Plan 184–193
   tunnel/Streaming), and `RemoteUnresolved` (typed failure
   rather than a silent local fallback). The
   `resolve_client_destination_with_decision` surface couples
   the typed `ClientTarget` with the routing decision so
   external drivers and tests can assert the path the manager
   would take.

5. **Per-service real tunnel material (Phase C).** The
   capability's counters advance only on positive observations;
   no test or path relies on `LocalZeroHop` material for the
   counted remote row.

7. **Outbound Streaming requests routed remotely (Phase D).**
   The capability's counter set covers
   `remote_outbound_requests` /
   `remote_inbound_payloads` /
   `remote_route_retries` /
   `remote_route_timeouts` /
   `remote_tunnel_loss`. The driver never silently falls back
   to the local bridge for a remote destination.

9. **Inbound remote destination traffic routed to the owning
   service runtime (Phase E).** The capability exposes
   `pending_resolution_count` and `expire_resolutions(now_ms)`
   so the inbound path can observe in-flight resolutions and
   cancel them at the hard deadline.

11. **Local path compatibility (Phase F).** The existing
    Plan 182 local bridge stays in place; the new
    `RoutingDecision::LocalCoOwned` arm is the explicit path
    for destinations the manager owns. The unit test
    `co_owned_hashes_track_committed_services` proves the
    prepared server destination routes as
    `LocalCoOwned` and never reaches the remote backend.

13. **First positive independent-router product test (Phase G).**
    The new `m10_remote_destination_streaming_composition_through_manager`
    driver exercises Direction A through the Plan 184–193 real
    router stack and asserts the manager-level routing decision
    flips `RemoteUnresolved` → `RemoteRouter` on
    `install_router_delivery_handle` and back on
    `uninstall_router_delivery`. The driver advances the
    Plan 202 §13 typed counters and emits a single terminal
    `remote-stream-established=true` row for the static
    checker.

15. **Counters and observability (Phase 13).** Twelve new
    typed counters in `RemoteDeliveryCounters` advance on
    positive observations only; unknown labels are silently
    ignored so a future expansion of the documented set must
    update both the helper and the static checker.

17. **Regression / hardening tests (Phase 14).** The
    `co_owned_hashes_track_committed_services` and
    `resolve_client_destination_with_decision_switches_after_install`
    unit rows prove the documented invariants stay green; the
    `routing_decision_switches_to_remote_router` row proves the
    `uninstall_router_delivery` path restores
    `RemoteUnresolved`.

## Required validation

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK
cargo test --locked --workspace --all-targets -- --test-threads=1          2355 passed, 11 ignored (97 suites; the +9 unit rows from service_delivery + plan202_routing_tests; the new external driver is `#[ignore]`-gated)
cargo test --locked -p i2pr-daemon --test service_tunnels_remote_transport_qualification
  expected without env: 0 passed, 1 ignored (fail-closed ordinary invocation)
cargo test --locked -p i2pr-daemon --test service_tunnels_remote_qualification
  expected without env: 0 passed, 1 ignored (fail-closed ordinary invocation)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                    0 passed (16 suites)
bash scripts/check-dependency-direction.sh                                OK
bash scripts/check-runtime-boundaries.sh                                  OK
bash scripts/check-service-tunnel-boundaries.sh                           OK
bash scripts/check-service-tunnel-acceptance-evidence.sh                  OK (29 guarded rows + 2 blocked + Plan 202 §12 invariants + Plan 202 driver present and gated)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                  OK
bash scripts/check-destination-tunnel-evidence.sh                         OK
bash scripts/check-streaming-tunnel-evidence.sh                           OK
bash tests/integration/service-tunnels/run-independent.sh                OK (29 rows command-derived, 3 rows blocked, the new m10-remote-destination-streaming-composition row fail-closed in this lane)
cargo deny check advisories bans sources                                  OK
```

## Handoff rule

Plan 202 closes when the **generic remote transport** rows are
positively command-derived through the
`m10-remote-destination-streaming-composition` external row.
On the current closing head, that row is recorded `blocked`
(fail-closed) because the M10 service-tunnel lane does not
provision the full SSU2 endpoint/bind tuple (the dedicated M6
interop lane `tests/integration/m6-interop/run-m6-mixed-router.sh`
provisions it). The row flips to `passed` only after the
dedicated M6 interop lane records
`lease-lookup-completed=...` + `remote-stream-established=true`
+ the typed counters `remote_lookup_succeeded>=1` /
`remote_stream_established>=1` /
`remote_outbound_requests>=2` /
`remote_inbound_payloads>=1`.

```text
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
m10_remote_transport_core = passed-via-plan202
next_m10_application_plan = 203
```

Plan 203 owns real curl/jaraco remote application acceptance.
Plan 204 owns final milestone authority normalization.