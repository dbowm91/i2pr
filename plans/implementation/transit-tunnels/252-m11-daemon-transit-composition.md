# Plan 252 — M11 daemon transit composition

Status: **registered-m11-daemon-transit-composition-ready**

Baseline: Plan 250 implementation `44187ce` (ordinary CI green; see Plan 250 status).

## Objective

Compose the corrected runtime-neutral M11 transit participant into the authenticated
daemon router-I2NP path. Boundedly admit ShortTunnelBuild work, forward accepted builds
and encrypted replies through the existing router-delivery seam, route TunnelData by
receive id to the authenticated next hop, and reclaim all participant state on expiry,
terminal delivery failure, cancellation, and shutdown. This is daemon infrastructure; it
does not qualify M11 capability or interoperability.

## Why this plan is ready

- Plan 250 closes the runtime-neutral semantics and ownership contract, including the
  move-only pending reservation token, `TransitBuildContext::previous_peer`, sealed
  code-30 policy response, and `TransitRegistry` role lookup/removal. Its implementation
  SHA `44187ce` has green ordinary GitHub Actions run 36060781701.
- Plan 251 closes the ordinary-CI Java source-lock gate on `6cf441d`; its CI run
  36050553519 is green.
- The daemon already receives `Ssu2InboundI2np` with authenticated peer/link metadata,
  classifies ShortTunnelBuild and TunnelData in `router_i2np`, and has bounded outbound
  `RouterDeliveryService::deliver` backed by `Ssu2RuntimeService::send_i2np`.
- `ExploratoryBuildCoordinator` is the current serialized build owner and handles
  creator-build correlation. Plan 252 must compose transit state alongside it without
  confusing transit ids/roles with creator tunnel state.
- No unresolved architecture decision is needed: daemon/runtime own scheduling,
  cancellation and sockets; `i2pr-tunnel` remains runtime-neutral. No advertisement,
  listener exposure, dependency, or wire-format changes are authorized.

## Current implementation evidence

- `crates/i2pr-daemon/src/router_i2np.rs`: `dispatch_router_i2np` preserves the
  authenticated peer/link; ShortTunnelBuild currently returns `TunnelBuildReserved`,
  TunnelData returns its receive tunnel id, and `RouterDeliveryService` sends through
  the established SSU2 service without a task per delivery.
- `crates/i2pr-daemon/src/exploratory_build.rs`: bounded `ExploratoryBuildCoordinator`
  routes existing creator-build replies and forwarded inbound-build correlations.
- `crates/i2pr-daemon/src/service_product.rs`: daemon composition currently owns the
  SSU2 handle, router delivery, and exploratory coordinator under explicit product
  lifecycle ownership.
- `crates/i2pr-tunnel/src/transit.rs`: Plan 250 provides transactional processing,
  admission reservations, sealed responses, a receive-id registry, next-hop metadata,
  role transforms, and typed removal/expiry primitives.

## Invariants

- Only authenticated `Ssu2InboundI2np::peer` establishes the previous peer. Never accept
  a peer identity supplied by decoded request data or a caller outside the authenticated
  ingress value.
- Admission and queue work remain bounded globally and per peer. Reserve before expensive
  processing, release on every rejection/error/cancel path, and commit only after the
  participant registration is installed and its response is ready for delivery.
- Preserve the sealed response bytes produced by the tunnel crate. Do not reconstruct or
  reseal a reply in the daemon.
- Transit role keys, decrypted build records, and payload bytes stay out of Debug/logs.
- Registry operations and task state have explicit owners. Do not add sync primitives to
  `i2pr-tunnel`, spawn one task per cell, or create detached/ownerless tasks.
- Duplicate receive ids fail closed; never evict an unexpired participant to admit a new
  one. Expiry is driven by caller-supplied time and lifecycle cleanup is deterministic.
- Transit delivery uses the existing bounded router queue/resource-governance path and
  remains lower priority than router-owned/client traffic.
- SSU2, SAM, I2CP, service tunnels remain loopback-only and disabled by default in normal
  configuration; no router version/capability advertisement changes.

## Scope

### In scope

- A daemon-owned transit composition component integrated into the authenticated
  router-I2NP processing owner.
- A bounded supervised/serialized lifecycle for inbound ShortTunnelBuild processing,
  outbound build/reply delivery, TunnelData forwarding, expiry, and shutdown.
- Correct delivery direction: accepted short-build continuation and sealed participant
  response go to their specified authenticated router peers; inbound TunnelData is
  accepted only from the registration's previous peer and forwarded to its next peer
  with the registered next tunnel id.
- Typed outcomes for admission rejection, malformed/fatal processing, queue/resource
  denial, missing link, duplicate/unknown receive id, expiry, and shutdown cleanup.
- Deterministic tests of actual daemon composition boundaries using bounded loopback or
  injected delivery seams, without claiming independent-router interoperability.
- Narrow daemon architecture/roadmap/support documentation updates describing the
  infrastructure state and explicitly preserving `advertised=false` / capability
  unclaimed.

### Explicitly out of scope

- Exact-pinned i2pd qualification, genuine external build traffic, repeated independent
  runs, or M11 completion. Those are Plan 253.
- Public-network transit, non-loopback listeners, capability/version/RouterInfo changes,
  broad bandwidth governance redesign, floodfill, legacy build compatibility, or new
  wire behavior.
- Changes to Plans 250/251 semantics or reference pins.

## Required production changes

1. Add daemon composition state for the Plan 250 transit registry/admission owner, with
   explicit policy/config defaults that keep participation disabled unless the controlled
   M11 test/product path opts in. Avoid adding public configuration or enabling it in
   ordinary product profiles without a documented existing controlled opt-in.
2. Route authenticated ShortTunnelBuild input into `process_transit_build` only when
   transit is enabled. Supply `previous_peer` exclusively from inbound authenticated
   metadata and pass caller time, local hop identity, admission limits and crypto inputs
   through typed interfaces.
3. Deliver the exact sealed response and accepted forwarding message through
   `RouterDeliveryService`. Define terminal handling for Accepted, QueueFull,
   ResourceDenied, NoActiveSession, deadline, cancellation, and oversize outcomes. Failed
   response/continuation delivery must remove or roll back the participant registration
   according to whether the build can still be safely represented; never leave an
   unobservable half-committed hop.
4. On TunnelData, look up the receive id, authenticate that the sender matches the
   registration's previous peer, apply the role's bounded transform, and send one
   correctly addressed cell to the registered next hop. Reject wrong-peer, unknown-id,
   expired, malformed, or replayed input without state mutation beyond documented replay
   protection.
5. Add one bounded expiry/lifecycle driver owned by the daemon service scope. It must
   remove expired transit registrations and drain all transit state before service
   shutdown completes. No per-cell task spawning or unbounded channels.
6. Keep creator-side `ExploratoryBuildCoordinator` routing intact. Dispatch ordering must
   not classify a forwarded creator reply as a fresh transit request when it matches an
   existing pending creator attempt.

## Ordered work packages

1. Trace the live router-I2NP receive owner and service shutdown path; write down the
   serialization/ownership point for transit state before editing.
2. Implement a small daemon-owned transit processor and typed outcome/error mapping using
   the Plan 250 APIs. Keep decoding and admission in `i2pr-tunnel`; keep runtime policy,
   queueing, and lifecycle in the daemon/runtime layer.
3. Integrate ShortTunnelBuild and TunnelData dispatch while preserving creator-build
   reply correlation and the authenticated ingress metadata.
4. Add explicit expiry tick and cancellation/shutdown drain to the existing supervised
   owner; make tick cadence/deadlines bounded and testable with supplied time.
5. Add deterministic composition and lifecycle tests; update static boundary checks only
   where they enforce a newly introduced invariant, without broad exclusions.
6. Update the M11 roadmap, daemon architecture deep-dive, support inventory, registry,
   and this plan's closure record with actual verification evidence.

## Failure, cancellation, restart, and contention semantics

- Invalid/unauthenticated ingress fails closed before transit state allocation. Valid
  policy denials use the sealed code-30 response from the tunnel core and do not install
  state.
- Any fatal build processing, serialization, or pre-commit delivery failure releases its
  reservation and leaves no registration. After registration, a terminal delivery error
  must synchronously remove the entry and drop its secret owners before returning.
- Cancellation before delivery prevents queue admission. Cancellation/shutdown after
  registration drains the registry under its single owner; it must not detach cleanup.
- Queue/resource contention is explicit and bounded. QueueFull/ResourceDenied never
  trigger retry loops or extra buffering; report a typed terminal result and clean up the
  affected participant when delivery is required for correctness.
- Expiry compares supplied `now_ms` against exact registered expiry, removes once, and
  drops keys. No wall-clock sleeps in state-machine tests.
- Process restart begins with an empty in-memory transit registry; there is no persistence
  or replay of transit secrets.

## Compatibility and migration

No wire codec, external API, RouterInfo, protocol version, dependency, or persisted format
changes are expected. Existing default-disabled behavior must remain unchanged. The daemon
must continue to preserve creator-build routing, service-tunnel traffic, and ordinary
router I2NP dispatch. Plan 253 will consume this local composition as its test subject.

## Required tests

- Authenticated previous-peer value is passed through and a spoofed/different peer cannot
  install or use a transit registration.
- Valid accepted ShortTunnelBuild produces exactly the core-sealed response and the
  correct next-hop continuation; policy denial returns the sealed code-30 response with
  no installed state.
- Full global/per-peer pending/active limits, duplicate receive id, queue pressure,
  resource denial, and each fatal construction/delivery path return counters and registry
  to their expected baselines.
- TunnelData success uses receive-id lookup, previous-peer check, participant transform,
  and correct next peer/tunnel id; unknown id, wrong peer, expiry, malformed, duplicate,
  and replay paths fail closed.
- Expiry tick removes each expired entry once. Cancellation and normal shutdown drain all
  registrations and release their secret owners. Restart starts empty.
- Creator-build reply correlation remains unchanged when matching and nonmatching
  ShortTunnelBuild messages enter the same dispatcher.
- Lifecycle/queue tests demonstrate there is no task-per-cell growth and all retained
  queues are bounded.

## Exact verification commands

Run from repository root, serializing loopback tests:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
git diff --check
```

Also run each new/changed focused integration target exactly and inspect the ordinary CI
run on the implementation SHA. Do not run Plan 253's external i2pd lane as a substitute
for Plan 252 closure evidence.

## Documentation updates

- `plans/subsystems/transit-tunnels-roadmap.md`: Plan 252 result, remaining Plan 253 gate,
  and exact capability boundary.
- `docs/architecture/i2pr-daemon.md`: transit owner, dispatch path, lifecycle and
  queueing boundaries.
- `specs/support.toml` and `specs/protocols/05-tunnels.md`: infrastructure status only;
  `advertised=false`, transit capability unclaimed until Plan 253 and M11 closure.
- `plans/registry.md` and `plans/closure/transit-tunnels/252-status.md`: close this plan
  and perform the successor unblock audit.

## Acceptance criteria

- All required tests and verification commands pass on the implementation SHA, and
  ordinary CI is green on that exact SHA.
- Authenticated provenance, bounded admission/delivery, exact reply forwarding,
  receive-id/previous-peer next-hop dispatch, expiry, cancellation and shutdown cleanup
  are proven at the daemon composition boundary.
- Existing creator builds and daemon routing remain green; no detached task, unbounded
  channel, new sync primitive in `i2pr-tunnel`, or secret-bearing debug/log output is
  introduced.
- No capability, interoperability, or advertisement claim is made. Plan 253 is unblocked
  only if all required interfaces are stable and no unforeseen protocol/architecture issue
  is discovered.

## Stop conditions

Stop and record a typed blocker if implementation discovers any of the following:

- The Plan 250 contract cannot express a real reference-compatible transit transition
  without changing its semantics; do not patch around it silently.
- Correct authenticated next-hop delivery requires changing wire/addressing behavior or
  accepting unauthenticated peer provenance.
- The bounded router-delivery/resource-governance seam cannot support the required
  serialization/lifecycle guarantees without redesigning unrelated service traffic.
- A spec ambiguity requires changing capability advertisement, legacy build support, or
  public-network policy. Register a separate decision/corrective plan before proceeding.

## Closure evidence required

The closure record must include implementation SHA, exact CI run, test/command results,
requirement-to-test matrix, any deviations, dependency changes, secret/resource-boundary
review, and an unblock audit for Plan 253 and any newly surfaced blockers. Plan 253 may be
registered only when these gates are proven, otherwise preserve its blocked/unregistered
state with a concrete reason.

## Handoff notes

Plan 250 supplies runtime-neutral single-owner state; Plan 252 chooses and implements the
daemon owner. Preserve the distinction between a local hop identity and the authenticated
previous peer. Treat `TransitBuildOutcome::Accepted`'s sealed response as opaque wire
material. Do not claim router interoperability from local composition tests. Current
baseline is main at `44187ce`, with Plan 250 ordinary CI green on Actions run 36060781701.
