# Plan 317 — Normal-daemon Destination-group pool provider

Status at registration: **ready**. Current status: **blocked**; corrective successor: Plan 318.

Classification: daemon/router product capability foundation.

Hard dependencies: Plan 314 passed; Plan 315 passed; Plan 316 blocked closure recorded; ADR 0030 accepted.

## 1. Objective

Compose the validated peer selector and Plan 315 Destination-group tunnel pools into the normal daemon's existing router/SSU2 owner. Publish a typed operational-readiness signal only after required group pools are usable, so Plan 316 can add lifecycle and shutdown ownership without creating a second transport stack or enabling listeners prematurely.

## 2. Why ready

Plan316's source audit established a specific missing boundary: the normal daemon neither calls `register_service_tunnel_manager` nor owns a provider that turns its validated router state and inbound I2NP path into Plan315 group pools. The separate `ServiceProduct` qualification composition already contains the selector and pool implementation, but it constructs another SSU2 service and uses a test child scope. The next work can be bounded around transferring these existing owners into normal daemon composition; no ADR change is needed.

## 3. Current implementation evidence

- `build_daemon_graph_inner` does not consume `Config::service_tunnels`.
- `register_ssu2_service` is the normal SSU2 owner and currently dispatches inbound messages without a Destination tunnel/group-pool product.
- `ServiceProduct::start` creates its own `Ssu2DaemonService`, `DestinationTunnelCoordinator`, `ExploratoryBuildCoordinator`, and test child scope; it is called only by qualification integration tests.
- Plans 314–315 provide exact-three peer selection, deterministic build proof, canonical group pools, and LeaseSet/data/lookup/publication consumers.
- Plan316 is blocked until this shared production provider exists.

## 4. Invariants

1. The normal daemon has exactly one owner of each configured SSU2 socket and one owner of each inbound I2NP handoff queue.
2. Group build selection consumes only validated, bounded RouterInfo peer facts and fails closed when diversity or pool requirements are not met.
3. No service listener admission or network publication precedes router operational readiness and the configured group's usable pool threshold.
4. Plan315 group pools remain the sole owner of service LeaseSet/data/lookup/publication tunnel material; no shadow pool or per-service pool is introduced.
5. The current SSU2 profile remains loopback-only, non-advertised, and disabled by default. This plan makes no external interop or anonymity claim.
6. Task ownership, cancellation, resource bounds, and privacy-safe diagnostics remain explicit.

## 5. Scope

Integrate the router-backed Destination-group provider with the existing normal daemon SSU2 service and inbound pump; source validated peer facts from the daemon's authoritative bounded NetDB/bootstrap owner; use the Plan314 selector and Plan315 group pool/runtime consumers; expose typed readiness to service composition; and wire `Config::service_tunnels` to a supervised daemon product behind its existing disabled-by-default switch.

Out of scope: graceful shutdown/drain behavior (Plan316), reworking unrelated router services, non-loopback advertisement, public-network acceptance, Streaming tuning, and production-anonymity claims.

## 6. Ordered work packages

WP1 define the typed router-ready/group-pools-usable snapshot and ownership handoff; WP2 construct the router-backed group provider over the existing SSU2 service and inbound I2NP owner; WP3 connect validated NetDB candidates to bounded Plan314 selection and Plan315 replenishment/expiry/consumer paths; WP4 register configured service groups in the normal graph and gate listener/publication startup on readiness; WP5 add deterministic lifecycle, failure, cancellation, and no-second-transport regressions; WP6 update architecture, configuration, and operational documentation.

## 7. Failure, cancellation, restart, and contention

Insufficient validated peers, build failure, pool shortfall, or router readiness loss must keep affected groups unpublished and not accepting. A bounded retry must not bypass the selector or duplicate pending builds. Cancellation drains every owner through the normal daemon's scopes. Restart reconstructs groups from persistent Destination identity and fresh validated peer/pool material. Group ceilings must fit the daemon's existing task, tunnel, and buffer budgets.

## 8. Compatibility and migration

No new required config. Service tunnels remain disabled by default and loopback-only. Existing enabled service configuration must either become a real readiness-gated daemon product or fail explicitly during startup; silently accepting configuration without an owner is forbidden. No identity migration is allowed without a versioned, recoverable path.

## 9. Required tests

Single SSU2 socket/dispatcher ownership; validated peer source and selector diversity; exact-three path submission through the normal owner; usable-pool readiness and minimum thresholds; no listener/publication before readiness; all group consumer routes use Plan315 pools; build failure and insufficient peers fail closed; restart and cancellation cleanup; capacity and queue ceilings; service-tunnels disabled-by-default behavior; normal graph composition and service-tunnel boundary checks.

All time-sensitive state tests use Tokio paused/manual time. No public I2P peers, system networking, or wall-clock drain tests.

## 10. Exact verification commands

Focused daemon product and group-pool tests; `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1`; the workspace floor; `bash scripts/check-dependency-direction.sh`; `bash scripts/check-runtime-boundaries.sh`; `bash scripts/check-service-tunnel-boundaries.sh`; and relevant Plan314/315 evidence checks.

## 11. Documentation updates

Document normal-daemon router/group readiness ownership, validated peer-to-pool flow, listener/publication activation gates, current loopback-only limits, and the composition API consumed by Plan316. Update daemon and runtime architecture deep-dives and the anonymity roadmap.

## 12. Acceptance criteria

The normal daemon composes configured Destination groups over its single router/SSU2 owner; group peer selection and tunnel consumers use the Plan314/315 implementation; no group admits application traffic or publishes without usable pools; and deterministic tests prove all readiness, failure, ownership, restart, and cleanup transitions. Plan316 becomes dependency-ready after this plan passes.

## 13. Stop conditions

Stop if the normal daemon cannot provide one bounded authoritative validated RouterInfo source, if integrating group traffic requires a second SSU2 owner or split inbound dispatcher, if the current runtime cannot safely retain group delivery owners, or if pool provisioning requires weakening the three-hop/diversity contract.

## 14. Closure evidence

Record the normal daemon owner/call graph, typed readiness transition table, proof that no duplicate router/transport owner exists, tests for each group-pool consumer and every negative gate, exact commands and outcomes, compatibility/config behavior, security review, and the Plan316 unblock audit.

## 15. Handoff

After pass, resume Plan316 for lifecycle and graceful-shutdown integration. Plan312 and Plan313 remain independent.
