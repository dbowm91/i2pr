# Plan 316 — Daemon-owned service-group lifecycle integration corrective

Status at registration: **ready**.

Corrects the integration boundary identified by Plan 311. Plan 311 remains an immutable blocked closure record; this plan owns the runtime and daemon composition changes required before its lifecycle requirements can be implemented against a production consumer.

Classification: daemon/runtime lifecycle capability.

Hard dependencies: Plan 315 passed; ADR 0030 accepted; Plan 311 closure recorded.

## 1. Objective

Make the Plan 315 Destination-group pool owner part of an explicitly supervised daemon service product, with router-readiness-gated activation and a bounded graceful drain that is separate from hard shutdown and per-connection cleanup.

## 2. Why ready

Plan 315 supplies the group-owned pool and consumer interface. Plan 311's audit established the missing integration contract: the normal service-tunnel graph starts listeners without router-backed group readiness, while supervisor shutdown cancels service tasks before any group-retirement phase. The gap is now explicit and can be implemented without changing ADR 0030.

## 3. Current implementation evidence

- `ServiceProduct` provisions group material before starting service supervisors, but remains a separately constructed qualification composition.
- `register_service_tunnel_manager` prepares and starts M10 service listeners without a router operational-readiness or Plan 315 group-pool gate.
- `Supervisor::shutdown` immediately cancels root and active service tokens and caps its task join deadline at `MAX_SHUTDOWN_DEADLINE` (30 seconds).
- Service loops currently share cancellation between listener ownership and active connections, so graceful admission stop and existing-stream continuation are not independently represented.

## 4. Invariants

1. No network publication or application admission precedes router operational readiness and usable group pools.
2. Graceful drain stops admission, pool replacement, and LeaseSet refresh while retaining required transport/delivery machinery for already-published leases and existing streams.
3. Hard and fatal-security shutdown bypass the long drain.
4. All task ownership and deadlines remain explicit and bounded; the group retirement ceiling is at most eleven minutes.
5. The short per-connection shutdown timeout is unchanged.
6. Lifecycle diagnostics expose only coarse local state and time buckets.

## 5. Scope

Integrate the Plan 315 group owner into daemon composition; define typed router readiness and group activation; split admission-loop cancellation from active-connection cancellation; add a graceful pre-shutdown phase to daemon/runtime composition with a hard-shutdown upgrade; stop pool replenish/publication refresh during retirement; retain transport until drain completion or deadline; add coarse local status.

Out of scope: Streaming fingerprint tuning, public anonymity claims, changing the eleven-minute upper bound, and unrelated router shutdown redesign.

## 6. Ordered work packages

WP1 define the router readiness and typed group lifecycle contracts; WP2 compose group pools under the daemon-owned service runtime; WP3 split admission stop from active-connection cancellation with explicit child ownership; WP4 add graceful/hard shutdown orchestration and bounded drain; WP5 add manual-time state, restart, cancellation, and deadline tests; WP6 update architecture and operational documentation.

## 7. Failure, cancellation, restart, and contention

Readiness failure leaves services unpublished and not accepting. Hard shutdown upgrades graceful shutdown immediately. Group removal does not initiate router-wide drain. Cancellation and forced teardown release every listener, driver, tunnel registration, and child task. Restart reconstructs group state from persistent identities and fresh tunnel material; it does not reuse retired leases.

## 8. Compatibility and migration

No required config change. Immediate stop remains available. Any new lifecycle profile is defaulted only after deterministic tests establish its timing contract. Existing per-connection deadlines retain their current meaning.

## 9. Required tests

Readiness and usable-pool gates; inbound-first/outbound-after-one-second sequencing under manual time; admission stops while established streams continue; no replacement or publication refresh after Retiring; zero-group graceful exit; natural expiry; eleven-minute hard cap; hard/fatal bypass; hard upgrade; restart; child-task cleanup on every terminal path.

## 10. Verification

Run focused runtime/daemon/service lifecycle tests, the workspace floor, and runtime/service boundary checkers. All timing tests use paused/manual time; no wall-clock ten-minute test.

## 11. Documentation

Document the readiness contract, graceful versus hard shutdown, transport-retention behavior, the distinction between Java's router-level reference and i2pr's service-group policy, and the limit of the scoped timing-correlation goal.

## 12. Acceptance criteria

The normal daemon composition consumes the Plan 315 group owner; publication and listeners are readiness-gated; graceful shutdown drains groups without canceling extant service streams before their bounded completion; hard/fatal shutdown remains prompt; manual-time tests prove every state transition and deadline.

## 13. Stop conditions

Stop if safe transport retention cannot be separated from listener and active-stream ownership, if supervisor integration would leak child tasks, or if the lifecycle policy requires an unbounded runtime deadline.

## 14. Closure evidence

Record the normal daemon call graph, lifecycle transition table, manual-time traces, shutdown ownership proof, exact command outcomes, compatibility decisions, and security review.

## 15. Handoff

Once passed, revisit Plan 311 as the lifecycle acceptance target. Plan 312 remains independently executable and is not gated by this corrective.
