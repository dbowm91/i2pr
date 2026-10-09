# Plan 390 — Destination role registry capacity corrective

Status: **blocked-post-registration-inbound-owner-installation-plan-391**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Corrects Plan 389's localized failure. The exact-pinned NONE lane completed
inbound build material for the control-created server, admitted it to the
Destination pool, and activated its tunnel, but every role activation failed.
The shared coordinator's `DataPlaneRegistry` capacity is derived only from the
small exploratory pool maxima, despite also receiving post-start
Destination-owned roles.

## Objective

Define and implement an explicit bounded capacity and lifecycle for role
projections owned by the shared coordinator. The inbound registry retains one
router role per service-pool inbound lease for routing, so the bound must cover
all supported service Destination groups plus the exploratory pool. Outbound
service roles are moved immediately into their owning bridge, but activation
needs one transient slot in addition to the exploratory pool. Requalify the
post-start reverse ELS2 NONE row. Register a further successor if a new runtime
boundary is localized.

## Dependencies and evidence

Hard inputs: Plans 380/381 for the frozen consumer credential and pinned lane;
Plans 385–389 for post-start authority, publication, readiness, fair scheduling,
and owner-scoped transition localization. Plan 389 closure records the evidence
and root cause. `DataPlaneRegistry` is shared by exploratory and Destination
roles; `DestinationRuntime` pools remain the canonical owner of per-Destination
registration metadata and established material.

## Invariants

1. Use unmodified stock i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5` and `MAX_ATTEMPTS=1`.
2. Keep all payload traffic on loopback SAM and the live private mesh.
3. Every registry bound must be finite, explicit, checked before allocation,
   and derived from documented maxima for Destination groups and roles; no
   unbounded role map and no capacity based only on `u8::MAX` without an
   architectural maximum.
4. Keep pool registrations and expiry/removal authoritative in each
   `DestinationRuntime`; role activation is a routing projection, never a
   second owner of tunnel secrets or pool metadata.
5. Remove every role projection on pool eviction, replacement, generation
   retirement, and shutdown. Preserve the exploratory pool's configured
   capacity and semantics.
6. Preserve one coordinator, fair inbound/outbound build scheduling, pending
   bounds, retry/failure limits, cancellation ownership, and Plan 389's
   destination-scoped redacted counters.
7. Do not alter transcript semantics, Proposal 170 inventory, credential seam,
   support inventory, or advertisement posture.

## Ordered work packages

1. Trace role registration/removal: inbound roles remain in the coordinator
   registry until their receive tunnel is evicted; outbound roles move into
   the owning bridge immediately. Derive maxima from `MAX_SERVICE_TUNNELS` (32),
   `MAX_EFFECTIVE_DIRECTION_TUNNELS` (8), and `MAX_EXPLORATORY_INBOUND` /
   `MAX_EXPLORATORY_OUTBOUND` (8 each).
2. Use a finite combined capacity: inbound is exploratory maximum plus
   `MAX_SERVICE_TUNNELS * MAX_EFFECTIVE_DIRECTION_TUNNELS` (264); outbound is
   exploratory maximum plus one synchronous transient activation slot (9).
   Keep normal non-service coordinators sized only to their supplied pool.
   Check arithmetic and representation conversions.
3. Add regression coverage for the derived capacities, filling to max and
   rejecting max+1 without mutation, and verify inbound capacity release via
   receive/slot removal and generation retirement. Preserve public routing
   metadata consistency and secret ownership.
4. Correct the capacity transition and update the registry contract, daemon
   architecture deep-dive, and checker mutations. Keep diagnostics numeric and
   coarse; do not include role IDs, peer hashes, or error strings.
5. Run the clean exact-pinned reverse NONE lane. Only after NONE returns the
   expected payload, run PSK/DH, authority, and three-peer controls, then the
   relevant ELS2 guards and routine floor.

## Verification

Run `rtk cargo fmt --all --check`, the managed-app sibling build required by
AGENTS.md before any focused daemon test, focused registry/coordinator and
Destination lifecycle tests, ELS2 runner/checker self-tests, checker mutation
table, encrypted-consumer caller guard, and exact-pinned NONE. Full workspace
qualification is conditional on the live matrix succeeding.

## Acceptance and stop conditions

Pass requires role capacity to cover the supported simultaneous exploratory
and service-Destination maxima, reject max+1 deterministically, and release
every slot on all lifecycle exits. The control-created reverse server must
register the minimum usable inbound leases, install LS2, cross local
DatabaseStore admission, and return the stock NONE payload. Continue to PSK/DH
and the remaining matrix only after that pass.

If any existing configuration path can exceed the stated group or per-direction
maxima, stop and register a successor for that bound; do not substitute an
arbitrary large capacity.

## Lifecycle, compatibility, and security

No persistent format or public wire/control schema changes. Do not clone or
serialize secret material for registry accounting. Role projection capacity
must not outlive the Destination pool registration it represents. Failures
remain local to service-tunnel provisioning and cannot break router startup.
No capability or anonymity claim changes.

## Closure evidence required

Record the exact bound derivation, every register/remove path, regression
coverage and max+1 behavior, lifecycle/secret review, exact commands and
results, live artifact hashes for every executed row, and registry/roadmap
unblock audit.
