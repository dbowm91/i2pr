# Plan 311 — Service lifecycle startup separation and graceful drain

Status at registration: **blocked-on-plan310**

Classification: anonymity lifecycle capability + shutdown correctness.

Hard dependencies: Plans 309 and 310 passed; ADR 0030.

## 1. Objective

Decouple Destination-group availability from router process startup/shutdown so service uptime does not mechanically mirror router uptime, while preserving bounded shutdown and operator control.

## 2. Exact reference facts

At Java I2P 2.13.0 commit `9134f808337b401e8e53c73734c81fab04280c9d`:

- I2PTunnel is configured with `delay=-1`; `LoadClientAppsJob` waits until the router reaches `RUNNING` before launch.
- Newly-created client inbound pools start before outbound pools, with outbound delayed one second.
- `TunnelPool` uses a ten-minute tunnel lifetime.
- `Router.shutdownGracefully()` documents a zero-to-eleven-minute graceful shutdown and waits on last participating-tunnel expiration plus clock fudge.

These are lifecycle references, not proof of Java's anonymity intent.

## 3. Current implementation evidence

Current service composition prepares identities/listeners and provisions router material before starting service supervisors. Per-service `shutdown_timeout_ms` is a short I/O/drain deadline capped at 30 seconds and is not suitable as the long service-retirement window.

## 4. Invariants

1. Service publication does not begin before router operational readiness and usable group tunnels.
2. Graceful shutdown is distinct from hard shutdown.
3. A service-drain wait is always bounded.
4. Per-connection `shutdown_timeout_ms` remains a short I/O cleanup bound and is not repurposed into the long anonymity drain.
5. Hard/emergency shutdown remains available for safety and does not wait for anonymity smoothing.
6. No new service work is admitted after a group enters Retiring.

## 5. Scope

Group lifecycle states; router-readiness gate; initial pool startup sequencing; service activation gate; graceful group retirement; LeaseSet refresh stop; tunnel replacement stop; bounded natural lease/tunnel expiry; daemon shutdown ordering; manual-time tests; coarse local status categories.

Out of scope: indefinite padding, global timing resistance, transport shutdown redesign unrelated to service drain, or real-time ten-minute CI sleeps.

## 6. Required production changes

1. Add group lifecycle states such as `Staged`, `Warming`, `Active`, `Retiring`, `Drained`, and `Failed`.
2. Define `RouterOperationalReady` from existing router/runtime facts; no arbitrary wall-clock sleep alone qualifies readiness.
3. Do not expose network service publication until required inbound/outbound group-pool minimums are usable.
4. Stage initial pool creation with the selected compatibility timing contract; initial reference is Java's inbound-first/outbound-after-one-second behavior.
5. Add a separate graceful service-drain policy. On graceful router shutdown:
   - reject new local client-originated sessions;
   - stop creating replacement group tunnels;
   - stop refreshing LeaseSet publication;
   - keep transport/tunnel machinery alive for already-published server leases and existing streams;
   - retire as leases/tunnels expire naturally;
   - force final group teardown at a hard maximum of eleven minutes.
6. Target a normal ten-minute tunnel-lifetime drain while allowing earlier completion when no published/active group state remains.
7. Keep hard shutdown and fatal-security shutdown separate; they bypass the long drain.
8. Add local-only status using coarse lifecycle/remaining-time buckets without peer or Destination identity.

## 7. Ordered work packages

WP1 lifecycle state machine/manual clock; WP2 readiness/activation gate; WP3 initial pool sequencing; WP4 retiring LeaseSet/pool behavior; WP5 daemon graceful-shutdown orchestration; WP6 hard/failure teardown; WP7 deterministic timing/restart tests.

## 8. Failure, cancellation, restart, and contention

Cancellation propagates from router graceful/hard shutdown to groups. Graceful shutdown may be upgraded to hard shutdown. Reconfigure/remove of one service does not automatically trigger the router-level eleven-minute drain; group-level removal follows its bounded service policy unless part of router graceful shutdown. No new work is admitted once Retiring.

## 9. Compatibility and migration

No mandatory configuration change. A named anonymity-compatible lifecycle profile becomes the default for network service groups after qualification. Operators retain an explicit immediate/hard stop action.

## 10. Required tests

Router not ready => no service publication; pool not usable => no publication; activation after readiness; inbound/outbound one-second staging under manual time; graceful shutdown with zero groups exits promptly; published server group drains across simulated tunnel lifetime; hard cap at eleven minutes; existing streams complete within their own bounds; hard shutdown bypass; restart restores correct persistent/ephemeral group state; no new LeaseSet refresh/replacement after Retiring.

## 11. Exact verification commands

Run full workspace/clippy/docs floor plus runtime/daemon/client/tunnel/service lifecycle tests and boundary scripts. Timing tests must use deterministic/manual time.

## 12. Documentation updates

Document graceful versus hard semantics, the ten-minute normal tunnel lifetime and eleven-minute hard ceiling, and that lifecycle smoothing reduces simple uptime correlation but does not defeat a global timing adversary.

## 13. Acceptance criteria

Service activation is readiness-gated; graceful shutdown keeps necessary network machinery alive during bounded retirement; no replacement/publication occurs after retirement starts; hard shutdown remains available; deterministic timing tests and full workspace floor pass.

## 14. Stop conditions

Stop if graceful retirement requires keeping unsafe failed transports alive, if LeaseSet/tunnel ownership cannot determine completion, or if shutdown ordering would deadlock the runtime supervisor.

## 15. Closure evidence required

State-transition table, manual-time traces, shutdown-ordering proof, exact test results, and security review.

## 16. Handoff

A pass closes the lifecycle branch required by the future integrated anonymity successor.
