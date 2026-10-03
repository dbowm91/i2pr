# Plan 318 — Normal-daemon single-owner Destination-group product

Status at registration: **ready**.

Corrects Plan 317's unverified assumption that the qualification `ServiceProduct` can be transferred directly into the normal daemon graph. Plan 317's blocked closure records the source audit and partial fail-closed configuration guard. Plan 316 remains blocked until this plan supplies the production provider boundary.

Classification: daemon/router product capability.

Hard dependencies: Plan 314 passed; Plan 315 passed; Plan 316 blocked closure recorded; Plan 317 blocked closure recorded; ADR 0030 accepted.

## 1. Objective

Compose configured Destination groups, Plan 314 exact-three selection/builds, and Plan 315 canonical pools into the normal daemon as one supervised router product. The product must retain the daemon's sole SSU2 owner and sole inbound I2NP consumer, use the bounded validated bootstrap/NetDB source, and expose typed router/group readiness before any configured application listener or publication can activate.

## 2. Why ready

Plans 314 and 315 provide the selector, build request, canonical pool, and consumer operations. The normal daemon has an existing bounded bootstrap store and one SSU2 service. Plan 317's audit found that its separate qualification product cannot be transplanted: it starts another SSU2 service, creates `ChildScope::for_test`, and owns a manually driven inbound loop, while the normal graph already owns a distinct SSU2 pump. This plan makes that single-owner convergence the explicit production outcome.

## 3. Current implementation evidence

- `Bootstrap::store()` exposes the validated bounded bootstrap store, but `run_daemon` currently builds the service graph with `Config` only.
- `register_ssu2_service` creates and supervises the normal SSU2 owner and its single inbound pump in `crates/i2pr-daemon/src/lib.rs`.
- `ServiceProduct::start` in `crates/i2pr-daemon/src/service_product.rs` creates a separate SSU2 owner and `ChildScope::for_test`; qualification tests call it directly and manually advance `poll_inbound`.
- `register_service_tunnel_manager` has no normal graph call site. Enabled service tunnels are now rejected during graph construction until a production owner exists.
- `DestinationPeerCandidate` intentionally exposes only bounded selection facts. Any transport-address resolution needed for a selected first hop must be derived from the same validated store record, without broadening candidate diagnostics or creating a second NetDB authority.

## 4. Invariants

1. Exactly one configured SSU2 socket owner and one inbound queue consumer exist in the normal daemon.
2. Only validated, bounded RouterInfo material feeds Plan 314 selection; exact-three and peer-diversity gates remain fail-closed.
3. The Plan 315 Destination-group pool remains the sole authority for group tunnel material and its LeaseSet/data/lookup/publication consumers.
4. No listener admission or network publication occurs before typed router readiness and the configured group usable-pool threshold.
5. Every spawned task has explicit runtime ownership and cancellation; no test-only scope or manually polled qualification owner is used in production.
6. SSU2 remains loopback-only, non-advertised, and disabled by default. This work makes no public-network or anonymity claim.

## 5. Scope

Create a production composition boundary over the normal SSU2 handle, bootstrap/NetDB state, group manager, and inbound dispatcher; adapt validated RouterInfo access to build selection and selected-peer delivery; supervise pool advancement and the single inbound pump; wire normalized service configuration to that owner; expose typed readiness; reject startup atomically when enabled groups cannot become usable. Preserve qualification helpers only as adapters over the same owner where practical.

Out of scope: Plan 316 graceful drain and lifecycle policy; changing the exact-three/diversity contract; non-loopback networking; external multi-router interoperability; changes to service protocol behavior or anonymity claims.

## 6. Ordered work packages

WP1 define the production owner API, validated-store handoff, and typed readiness snapshot; WP2 route the normal inbound pump through the owner without splitting queue consumption or bypassing existing floodfill/router dispatch; WP3 resolve selected first-hop transport material from validated records and submit exact-three requests through the existing delivery owner; WP4 attach Plan 315 group pool lifecycle and consumers to normalized service configuration; WP5 gate listener/publication activation and fail atomically on scarcity, transport, pool, or cancellation failure; WP6 add deterministic single-owner, readiness, negative-gate, restart, and resource-bound regressions; WP7 update daemon/runtime architecture and operational docs.

## 7. Failure, cancellation, restart, and contention

Insufficient peers, invalid address material, failed builds, or pool shortfall keep groups unpublished and listeners closed. Pending work remains scoped and bounded by existing Plan 314 coordinator ceilings and each Plan 315 group's concurrency. Cancellation releases inbound registrations and group roles through the same owner. Restart reloads persistent Destination identity and fresh validated RouterInfo/pool material; it does not reuse ephemeral tunnel secrets or retired leases.

## 8. Compatibility and migration

No required configuration changes. Service tunnels remain disabled by default. Existing enabled configurations either reach the typed ready state through this production owner or fail graph/service startup explicitly; they must never be silently ignored. No identity format or router/service identity relationship changes.

## 9. Required tests

Single SSU2 socket and inbound-consumer ownership; bootstrap-store handoff and validated peer projection; selected first-hop address resolution; exact-three request order and no short fallback; readiness state transitions; usable pool gate; no listener/publication before readiness; Plan 315 consumer ownership; insufficient-peer/build/delivery failure; cancellation and restart cleanup; global/group capacity bounds; disabled-by-default and explicit enabled-config behavior; ordinary daemon graph composition. All timing tests use paused/manual time and controlled loopback fixtures.

## 10. Exact verification commands

Focused daemon product, bootstrap-store, and group-pool tests; `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1`; the workspace floor; `bash scripts/check-dependency-direction.sh`; `bash scripts/check-runtime-boundaries.sh`; `bash scripts/check-service-tunnel-boundaries.sh`; `bash scripts/check-service-anonymity-boundaries.sh`; and relevant Plan 314/315 evidence checks.

## 11. Documentation updates

Document the normal-daemon single-owner composition, validated store-to-selector-to-pool flow, readiness gates, failure behavior, enabled-config startup semantics, and loopback-only limits. Update `docs/architecture/i2pr-daemon.md`, the relevant runtime and service-tunnel deep-dives, and the anonymity roadmap.

## 12. Acceptance criteria

The normal daemon composes configured Destination groups over its sole SSU2 and inbound-message owners; Plan 314 selection and Plan 315 consumers are the live group path; group readiness is typed and observable; listeners and publication remain gated until usable pools exist; and deterministic tests prove ownership, failure, cancellation, restart, and resource limits. Passing this plan makes Plan 316 dependency-ready.

## 13. Stop conditions

Stop if a single queue consumer cannot preserve existing floodfill/router dispatch semantics, if validated selected-peer transport data cannot be obtained without weakening store validation or exposing private diagnostics, if the loopback-only transport cannot support the required controlled path, or if task/resource ownership would require a second SSU2 service.

## 14. Closure evidence

Record the production call graph, one-owner proof, validated-store source and selected-peer route, typed readiness transitions, all negative activation gates, consumer ownership, cancellation/restart/resource regressions, exact command outcomes, compatibility/security review, and the Plan 316 unblock audit.

## 15. Handoff

After pass, mark Plan 316 ready for daemon-owned group lifecycle integration. Plan 311 remains behind Plan 316. Plans 312 and 313 remain independent of this provider work.
