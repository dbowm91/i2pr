# Plan 254 — M11 live ingress/body-threading closure corrective

Status at registration:
**registered-ready-m11-live-ingress-body-threading-closure-corrective**

Baseline:
`6a53efaeed30087b3ef133028f7243897d72afba`

Corrects the remaining Plan 253 closure defects while retaining its successful
runtime-neutral transit data-plane, bounded peer index, role-correct build envelopes,
rollback, cancellation drain, and secret-ownership changes.

The exact-pinned i2pd qualification moves to unregistered Plan 255.

## Objective

Finish the real controlled daemon consumer for M11 transit.

Plan 253 materially improved the transit subsystem, but its own closure record admits two
conditions that directly violate its acceptance criteria:

1. the actual live authenticated SSU2/router-I2NP owner does not call
   `TransitOwner::dispatch_short_build`;
2. `plan253_short_build_payload` returns `&[]` because the canonical dispatcher does not
   hand the decoded ShortTunnelBuild body to the owner.

Current GitHub Actions run `36089834582` on the Plan 253 closure SHA is also red on both
Ubuntu and macOS at `cargo fmt --all --check`. Therefore Plan 253 cannot remain the final
daemon-composition authority.

Plan 254 must:

- thread the actual bounded ShortTunnelBuild body from the existing canonical I2NP decode
  path into the controlled transit owner without a second decoder;
- invoke that owner from the real inbound SSU2/router-I2NP pump when controlled transit is
  enabled;
- preserve creator/service ownership precedence for TunnelData and route transit-owned
  cells exactly once;
- complete OBEP semantic delivery and IBGW TunnelGateway ingress through existing daemon
  delivery seams;
- remove all Plan-253 live-owner placeholders;
- restore committed formatting/CI truth;
- reconcile planning/support authority.

This is a closure corrective, not external interoperability work.

## Classification

**Infrastructure corrective.**

No transit capability, RouterInfo capability, router.version change, public listener, or
public-network participation may be enabled by this plan.

## Retained Plan 253 work

Do not rewrite or duplicate these surfaces unless a direct regression requires it:

- `i2pr_tunnel::process_short_build_message`;
- `TransitHopRegistration::process_tunnel_data`;
- `TransitHopRegistration::process_tunnel_gateway`;
- bounded duplicate/replay and OBEP reassembly state;
- move-only `TransitHopMaterial`;
- `TransitPeerIndex` hard bound;
- complete STBM / OTBRM envelope wrappers;
- code-30 route preservation;
- all-non-Accepted delivery rollback;
- synchronous cancellation drain.

Plan 254 exists to attach those pieces to the actual live owner and to prove that
attachment.

## Why Plan 253 verification missed this

Plan 253 added a new `TransitOwner` type and a daemon integration test, but the test still
constructed `TransitOwner` directly. It did not traverse the runtime owner that receives
`Ssu2InboundI2np`.

The new owner also introduced an explicit placeholder:

```rust
fn plan253_short_build_payload(_outcome: &RouterI2npOutcome) -> &[u8] {
    &[]
}
```

so even a caller of `dispatch_short_build` could not process a real STBM.

The closure record correctly listed these facts under "Remaining risks" but still marked the
plan passed. They are not post-closure risks; they are unclosed acceptance requirements.

Finally, local formatting was reported green, but the committed SHA failed ordinary CI
formatting on both supported quality jobs. Plan 254 closure must use committed-SHA CI as
authority.

## Architecture constraint: one canonical decode, bounded body handoff

Do not solve the problem by decoding the same I2NP envelope again inside
`transit_owner.rs`.

The existing router-I2NP dispatcher remains the canonical decoder/classifier.

Introduce the narrowest typed handoff that lets the live owner access the already-decoded
build body. Acceptable shapes include:

```rust
struct RouterI2npTransitBuild<'a> {
    body: &'a [u8],
    message_id: u32,
    expiration_ms: u64,
    peer: PeerId,
    link_id: LinkId,
}
```

or an equivalent bounded owned/borrowed type derived directly from the canonical decoded
`I2npMessage`.

Requirements:

- body length remains bounded by existing I2NP limits;
- no unbounded retained payload queue;
- no third decoder;
- no payload logging;
- authenticated `peer` / `link_id` remain attached to the same decoded message;
- ShortTunnelBuild and OutboundTunnelBuildReply remain distinguishable;
- ordinary callers that only need `RouterI2npOutcome` do not need to retain large payloads.

Avoid casually removing `Copy` from a widely used classification enum merely to carry a
`Vec<u8>` if a narrow borrowed dispatch type is sufficient.

## Work package A — make the current gaps executable regressions

Add failing tests before fixing production wiring.

1. A valid authenticated ShortTunnelBuild passed through the current live inbound owner
   does not reach `TransitBuildService`.
2. `TransitOwner::dispatch_short_build` currently receives an empty body.
3. The current daemon-level integration test can pass without invoking
   `dispatch_short_build`.
4. A transit-owned TunnelData cell is not reachable through the real inbound pump.
5. An OBEP `TransitTunnelDataDispatch::Deliver` is not consumed by the live daemon owner.
6. IBGW `TunnelGateway` ingress has no live owner path.
7. Current committed source fails `cargo fmt --all --check`.

These regressions must remain represented in the final requirement-to-test matrix.

## Work package B — canonical decoded build-body handoff

Refactor `router_i2np.rs` so the canonical standard/short-transport decode can produce a
narrow transit build input without re-decoding.

Preferred flow:

```text
Ssu2InboundI2np
  -> canonical decode once
  -> validate expiration / size / type once
  -> ordinary classification metadata
  -> optional borrowed/typed ShortTunnelBuild body view
```

The body supplied to transit must be exactly the count-prefixed STBM body expected by
`process_short_build_message`, not the entire encoded I2NP envelope and not an empty shim.

Required tests:

8. standard-header STBM yields exact decoded body bytes + authenticated peer/link;
9. short-transport-header STBM yields the same typed body semantics;
10. OutboundTunnelBuildReply cannot be misclassified as inbound transit build;
11. malformed/expired/far-future input fails before transit sees any body;
12. maximum-size boundary remains enforced;
13. the new handoff performs one canonical decode only.

## Work package C — wire TransitOwner into the actual inbound owner

Identify the daemon/runtime owner that consumes `Ssu2DaemonHandle::next_inbound()` /
`Ssu2InboundI2np` and already owns the canonical router-I2NP dispatch.

Controlled-transit ordering for ShortTunnelBuild:

```text
authenticated inbound
  -> canonical router-I2NP decode/classification
  -> creator correlation check
  -> if controlled transit disabled: preserve TunnelBuildReserved
  -> if enabled and non-creator: TransitOwner::dispatch_short_build(real body)
  -> deliver accepted/rejected result through existing bounded RouterDeliveryService
```

Requirements:

- default/ordinary product construction remains disabled;
- controlled tests/qualification may enable transit through an internal constructor or
  explicitly test-only/qualification-owned option;
- do not add public RouterInfo advertisement;
- do not create a second socket or event loop;
- use the outer owner's real cancellation token, not a fresh token created inside
  `TransitOwner::dispatch_short_build`;
- the owner must observe and propagate terminal delivery result;
- session-close events remove any transit peer mapping associated with that session.

Remove `plan253_short_build_payload` completely. A renamed equivalent returning an empty
placeholder is forbidden.

Required tests:

14. disabled live owner preserves existing reserved behavior;
15. enabled live owner receives exact STBM body and invokes transit once;
16. authenticated peer/link seen by transit exactly match `Ssu2InboundI2np`;
17. creator-correlated build bypasses transit;
18. valid policy rejection is delivered through the live owner;
19. accepted build delivery Accepted leaves one registration live;
20. terminal delivery result rolls the registration back through the live owner;
21. outer cancellation token cancellation drains transit and prevents further delivery.

At least one test must begin with a real `Ssu2InboundI2np` and traverse the same production
owner method used by the controlled runtime. Direct calls to
`TransitOwner::dispatch_short_build` alone do not satisfy this work package.

## Work package D — real TunnelData ownership ordering

The live inbound owner must deterministically decide who owns each TunnelData receive id.

Required order:

1. creator/exploratory/service data-plane owner;
2. transit registry, only if controlled transit is enabled;
3. unknown -> fail closed.

The first successful owner consumes the cell. Never transform a cell twice.

The routing decision may use a typed ownership probe before mutating state. Do not attempt
the creator/service path, mutate it, then fall through to transit after a partial failure.

Required tests:

22. creator/service-owned receive id never reaches transit;
23. transit-owned receive id reaches transit exactly once;
24. unknown receive id mutates neither owner;
25. wrong authenticated peer fails in transit without falling back to another owner;
26. replayed transit cell is dropped once and is not retried through another owner.

## Work package E — complete OBEP semantic delivery

`TransitHopRegistration::process_tunnel_data` can now return
`TransitDataOutcome::Deliver { action }`. The live daemon owner must consume it.

Map canonical `RouterDeliveryAction` through existing daemon/router/service delivery
capabilities:

- LOCAL -> canonical local/router-owned inbound delivery path;
- ROUTER -> bounded direct router delivery;
- TUNNEL -> canonical TunnelGateway delivery to the target gateway/tunnel.

Do not invent a second delivery stack.

Required tests:

27. OBEP unfragmented LOCAL action reaches the canonical local consumer;
28. OBEP ROUTER action produces one bounded router delivery;
29. OBEP TUNNEL action preserves target gateway/tunnel exactly;
30. fragmented OBEP message emits one action only after completion;
31. duplicate OBEP cell does not produce a second action;
32. delivery failure is typed and does not corrupt transit registration/reassembly state.

If no canonical LOCAL consumer is available at this ownership layer, stop and register a
narrow delivery-interface decision rather than silently dropping LOCAL.

## Work package F — complete IBGW live ingress

The runtime-neutral data plane already exposes
`TransitHopRegistration::process_tunnel_gateway`. Wire the actual role-appropriate inbound
`TunnelGateway` path to it when the receive tunnel belongs to a transit IBGW.

Requirements:

- do not route IBGW TunnelGateway input through `process_tunnel_data`;
- preserve existing creator/service TunnelGateway ownership first;
- canonical IBGW processing may emit one or more bounded TunnelData cells;
- deliver every emitted cell through existing bounded router delivery;
- partial multi-cell delivery failure is explicit and bounded; do not retry indefinitely.

Required tests:

33. transit IBGW receives a real TunnelGateway through the live owner;
34. emitted cell bytes match the canonical runtime-neutral result;
35. creator/service-owned gateway bypasses transit;
36. unknown gateway tunnel id fails closed;
37. multi-cell fragmentation remains bounded;
38. cancellation stops subsequent cell delivery.

If the current daemon has no reusable typed TunnelGateway ownership seam, this is a stop
condition; register the narrow seam rather than adding ad hoc decoding.

## Work package G — cancellation/session lifecycle

`TransitOwner::dispatch_short_build` currently creates a fresh
`CancellationToken::new()`. Replace that with the real owner token/reference.

Requirements:

- one authoritative outer cancellation token;
- shutdown invokes transit drain before owner shutdown completes;
- drop remains a fail-safe, not the primary lifecycle mechanism;
- session close removes the bounded peer-index mapping;
- peer mapping is established only from authenticated active session state.

Required tests:

39. owner cancellation drains active/pending/peer state immediately;
40. session close removes mapped peer;
41. cancelled build delivery returns Cancelled and leaves no accepted registration;
42. repeated shutdown/drop remains idempotent.

## Work package H — CI and planning truth

Before closure:

- run `cargo fmt --all` and commit the formatted result;
- `cargo fmt --all --check` must be green on the committed SHA;
- ordinary GitHub Actions Ubuntu + macOS quality jobs must reach and pass tests;
- MSRV and dependency policy must be green;
- do not record a local pass as closure authority if the committed source fails CI.

Update authority surfaces only after implementation truth is known.

## Static guard updates

Extend `scripts/check-m11-transit-boundaries.sh` only for durable invariants:

- forbid `plan253_short_build_payload` and equivalent hard-coded empty transit build body;
- prove the production live-owner module is referenced by the actual inbound owner outside
  tests;
- forbid `CancellationToken::new()` inside the production short-build dispatch path;
- forbid second I2NP decode in `transit_owner.rs`;
- preserve no raw `LayerKeys` / `TunnelLayerTransform` use in daemon transit code;
- preserve move-only secret owner and bounded peer index.

Do not use the script as a substitute for the live-owner integration tests.

## Exact verification floor

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

Run all new focused live-owner integration tests explicitly.

Closure additionally requires ordinary GitHub Actions success on the exact committed SHA:

- Quality (ubuntu-latest);
- Quality (macos-latest);
- MSRV (Ubuntu);
- Dependency policy.

## Out of scope

- i2pd execution or qualification;
- changing exact i2pd pin `2.61.0`;
- public-network transit;
- RouterInfo/capability/router.version changes;
- redesigning the Plan 252 full-message cryptography;
- redesigning the Plan 253 runtime-neutral data plane;
- bandwidth-policy changes;
- floodfill;
- Java router compatibility work.

## Compatibility / migration

No persisted-state migration expected.

Default product behavior must remain unchanged unless the internal controlled-transit
composition is explicitly enabled.

No new dependency expected. Stop for review if one is proposed.

## Acceptance criteria

Plan 254 closes only when all are directly proven:

1. Plan 252 full-message build tests remain green;
2. Plan 253 runtime-neutral data-plane tests remain green;
3. no empty-body transit-build placeholder remains;
4. ShortTunnelBuild body comes from the one canonical decoded inbound message;
5. actual live authenticated SSU2/router-I2NP owner invokes controlled transit;
6. ordinary/default construction remains transit-disabled;
7. creator correlation precedes transit admission;
8. real owner cancellation token is used;
9. creator/service TunnelData ownership precedes transit ownership;
10. transit-owned TunnelData is processed exactly once;
11. OBEP LOCAL/ROUTER/TUNNEL semantic delivery is wired or a narrower explicit stop is
    recorded before closure;
12. IBGW live TunnelGateway ingress is wired or a narrower explicit stop is recorded before
    closure;
13. session-close updates the bounded peer mapping;
14. accepted/rejected build dispatches remain complete I2NP envelopes;
15. every terminal delivery result retains Plan 253 rollback semantics;
16. no duplicate decode or daemon-side crypto path appears;
17. full workspace verification is green;
18. committed source is rustfmt-clean;
19. ordinary CI is green on the exact closure SHA;
20. planning/support authority points at the actual next state;
21. no capability/version/advertisement/public-network change lands.

If criteria 11 or 12 hit their named stop conditions, Plan 254 does not pass. Register the
narrow missing interface plan and retain Plan 254 with the exact blocker.

## Stop conditions

Stop rather than broadening scope if:

- carrying the body requires a general unbounded payload-retention redesign;
- live ownership cannot be decided without changing creator/service registry contracts;
- OBEP LOCAL delivery has no canonical owner interface;
- IBGW TunnelGateway cannot enter the existing daemon through a typed seam;
- a second decoder/transport loop appears necessary;
- a new dependency or public config/capability surface is proposed;
- exact external-router behavior is needed to decide a local ownership contract.

## Closure and successor rules

On registration:

- Plan 253 becomes retained with corrective required via Plan 254;
- Plan 254 is the next executable plan;
- Plan 255 is reserved/unregistered for exact-pinned i2pd controlled transit qualification;
- M11 capability remains unclaimed/unadvertised;
- M12 remains deferred.

Plan 255 may be registered only after Plan 254 closes with green exact-SHA CI and the
unblock audit confirms the controlled live owner is real.

## Handoff notes

Start in the live inbound owner and `router_i2np.rs`, not in i2pd.

The first implementation milestone is a test that starts from a real
`Ssu2InboundI2np` containing a valid STBM and demonstrates that the exact decoded body
reaches the existing Plan 252/253 transaction through the production owner.

Delete `plan253_short_build_payload` early. Do not leave it as dead compatibility scaffolding.

Do not call the plan complete because `TransitOwner` exists. Completion requires a
production caller.

Fix formatting before closure evidence is recorded, then require green current-SHA Actions.
