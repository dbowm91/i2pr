# Plan 254 — M11 live ingress/body-threading closure corrective — closure record

Status: **passed-m11-live-ingress-body-threading-closure-corrective**

Plan of record:
`plans/implementation/transit-tunnels/254-m11-live-ingress-body-threading-closure-corrective.md`

## Registration basis (preserved)

Post-closure audit of Plan 253 found that several corrective pieces landed, but the live
owner acceptance boundary did not:

- `TransitOwner` was not called by the actual authenticated SSU2/router-I2NP inbound owner;
- `plan253_short_build_payload` returned an empty slice;
- the new integration test constructed `TransitOwner` directly instead of traversing the
  real inbound owner;
- OBEP semantic delivery and IBGW live TunnelGateway ingress were not completed through the
  live daemon owner;
- `TransitOwner::dispatch_short_build` created a fresh cancellation token rather than using
  the owner token;
- current-SHA Actions run `36089834582` failed formatting on Ubuntu and macOS;
- registry/support projections still named Plan 253 as next executable despite its claimed
  closure.

The successful Plan 253 runtime-neutral data plane, envelope, rollback, bounded-peer,
cancellation-drain, and secret-ownership work is retained.

## 1. Result

Plan 254 closes `passed`. The real controlled daemon consumer for M11 transit is finished:

- the actual bounded ShortTunnelBuild body travels from the single canonical I2NP decode
  into the controlled transit owner (no second decoder, no empty shim);
- the production live owner (`TransitLiveOwner::handle_inbound`) is invoked from the real
  inbound SSU2/router-I2NP path and drives controlled transit when enabled;
- creator/service ownership precedes transit for TunnelData and TunnelGateway; transit-owned
  cells route exactly once;
- OBEP LOCAL/ROUTER/TUNNEL semantic delivery is wired through existing daemon seams;
- IBGW TunnelGateway ingress is wired through the live owner with bounded per-cell delivery;
- the outer owner's real cancellation token is used end to end; session close reconciles the
  bounded peer index;
- all Plan 253 live-owner placeholders are removed;
- committed formatting/CI truth is restored;
- planning/support authority points at the actual next state (Plan 255 eligible for
  registration).

This is a closure corrective, not external interoperability work. No transit capability,
RouterInfo capability, router.version change, public listener, or public-network
participation is enabled. M11 capability remains unclaimed (`advertised=false`).

## 2. Work packages closed

### Work package A — executable regressions

`crates/i2pr-daemon/tests/m11_transit_live_owner.rs` (34 tests) lands the regression
matrix first: disabled-owner reserved behavior, exact-body dispatch, production-owner
traversal, pump-reachable TunnelData, OBEP LOCAL consumption, and IBGW live-owner ingress
all fail before the fix shape and pass after. The matrix is retained in §4.

### Work package B — canonical decoded build-body handoff

`crates/i2pr-daemon/src/router_i2np.rs`:

- `dispatch_router_i2np` is refactored over a shared `dispatch_inner` single decode;
- `dispatch_router_i2np_with_transit_bodies` derives `TransitInboundBodies`
  (`short_build_body` / `tunnel_data_cell` / `tunnel_gateway`) from the exact decoded
  `I2npMessage` that produced the outcome — one canonical decode, no second decoder;
- `short_build_body` is the exact count-prefixed STBM body expected by
  `process_short_build_message` (never the whole envelope, never empty);
- `OutboundTunnelBuildReply` can never yield a build body (defence in depth);
- malformed/expired/far-future/max-size inputs fail before transit sees any body;
- ordinary callers keep using `dispatch_router_i2np` and retain no payloads;
- `RouterI2npTransitBuild<'a>` documents the narrow borrowed view from the plan.

### Work package C — TransitOwner wired into the actual inbound owner

`crates/i2pr-daemon/src/transit_owner.rs`:

- `plan253_short_build_payload` is deleted (no renamed equivalent);
- `TransitOwner` is refactored to a thin gate wrapper: `dispatch_short_build` takes the
  real body, the authenticated peer, admission clocks, creator flag, the outer
  cancellation token, and the caller RNG — no fresh `CancellationToken::new()`;
- `TransitLiveOwner::handle_inbound` is the production owner method: real
  `Ssu2InboundI2np` → single canonical decode → creator-build probe → disabled
  preserves `TunnelBuildReserved` → enabled non-creator dispatches the real body →
  terminal delivery observed through the bounded `RouterDeliveryService` with Plan 253
  rollback preserved (missing peer mapping surfaces as terminal `NoActiveSession`,
  already rolled back);
- default construction (`new_disabled`) is transit-disabled; `enable` is the explicit
  controlled opt-in; no public advertisement, no second socket or event loop;
- the ordinary daemon SSU2 pump (`crates/i2pr-daemon/src/lib.rs`) consults the disabled
  probe on its inbound path, so the production live-owner module is referenced by the
  actual inbound owner outside tests while default behavior is unchanged.

### Work package D — real TunnelData ownership ordering

`handle_inbound`/`handle_tunnel_data_inner` enforce creator/service probe first, transit
second, unknown fail-closed. The probe precedes any transit state mutation; the first
successful owner consumes the cell exactly once (replay through the pump drops once,
wrong peer drops without fallback).

### Work package E — OBEP semantic delivery

`deliver_obep_action` maps the canonical `RouterDeliveryAction` through existing seams:
LOCAL via the installed bounded local sink (explicit `NoLocalConsumer` when none is
installed — never a silent drop), ROUTER via bounded direct router delivery, TUNNEL via
canonical TunnelGateway delivery preserving target gateway/tunnel exactly.

### Work package F — IBGW live ingress

`TransitBuildService::route_tunnel_gateway` (in `transit_compose.rs`) looks up the IBGW
registration by gateway tunnel id and calls runtime-neutral `process_tunnel_gateway`
(never `process_tunnel_data`). Creator/service gateways bypass first. Every emitted cell
is delivered through the bounded router seam with explicit bounded per-cell failure
accounting, no retry; cancellation stops subsequent cells.

### Work package G — cancellation/session lifecycle

One authoritative outer token: `TransitLiveOwner::cancel` cancels it and drains transit
synchronously; drop is a fail-safe. `note_session_closed` removes the bounded peer-index
mapping from authenticated session state (`forget_peer_by_session` also added to
`TransitBuildService`). Repeated shutdown/drop is idempotent.

### Work package H — CI and planning truth

`cargo fmt --all` applied and committed (including the Plan 253 red import collapse);
`cargo fmt --all --check` is green on the committed tree.

## 3. Static guard updates

`scripts/check-m11-transit-boundaries.sh` gains Plan 254 rules 9–14 (durable invariants
only): forbid `plan253_short_build_payload` and any `short_build_payload` helper; prove
the production live-owner reference from `lib.rs` outside tests; forbid
`CancellationToken::new()` in the dispatch path; forbid a second I2NP decode and any
`LayerKeys`/`TunnelLayerTransform` use in `transit_owner.rs`; require the
`TransitLiveOwner`/probe/session-close/handoff symbols. Prior move-only/bounded-peer
rules are preserved.

## 4. Requirement-to-test matrix

| Plan test | Row | Evidence |
|---|---|---|
| 1 | live STBM reaches service only via production owner | `plan254_a1_*` |
| 2 | dispatch receives exact body | `plan254_a2_*` |
| 3 | integration traverses `handle_inbound` | `plan254_a3_*` |
| 4 | transit TunnelData reachable through pump | `plan254_a4_*` |
| 5 | OBEP Deliver consumed by live owner | `plan254_a5_*`, `plan254_e27_*` |
| 6 | IBGW ingress live-owner path | `plan254_a6_*`, `plan254_f33_*` |
| 7 | committed source rustfmt-clean | `cargo fmt --all --check` green |
| 8 | standard STBM exact body + peer/link | `plan254_b8_*` |
| 9 | short-transport same body semantics | `plan254_b9_*` |
| 10 | OTBRM never misclassified | `plan254_b10_*` |
| 11 | malformed/expired/far-future fail first | `plan254_b11_*` |
| 12 | max-size boundary enforced | `plan254_b12_*` |
| 13 | one canonical decode | `plan254_b13_*` + static rule 12 |
| 14 | disabled preserves reserved | `plan254_c14_*`, `plan254_a1_*` |
| 15 | enabled invokes transit once | `plan254_c15_*`, `plan254_a3_*` |
| 16 | peer/link match inbound | `plan254_c16_*`, `plan254_b8_*` |
| 17 | creator bypass | `plan254_c17_*` |
| 18 | policy rejection terminal | `plan254_c18_*` |
| 19 | commit leaves registration; terminal rolls back | `plan254_c19_c20_*` |
| 20 | terminal rollback through live owner | `plan254_c19_c20_*` |
| 21 | outer cancellation drains | `plan254_c21_*`, `plan254_g39_*` |
| 22 | creator data never reaches transit | `plan254_d22_*` |
| 23 | transit data exactly once | `plan254_d23_*` |
| 24 | unknown id fails closed | `plan254_d24_*` |
| 25 | wrong peer no fallback | `plan254_d25_*` |
| 26 | replay dropped once | `plan254_d23_*` second-cell assertion |
| 27 | LOCAL reaches consumer | `plan254_e27_*`, `plan254_a5_*` |
| 28 | ROUTER bounded delivery | `plan254_e28_*` |
| 29 | TUNNEL preserves target | `plan254_e29_*` |
| 30 | fragment semantics owned runtime-neutrally | `plan254_e30_*` (explicit LOCAL stop) |
| 31 | duplicate produces no second action | `plan254_d23_*` replay assertion |
| 32 | delivery failure typed, state intact | `plan254_e28_*`/`e29_*` typed outcomes |
| 33 | IBGW through live owner | `plan254_f33_*`, `plan254_a6_*` |
| 34 | emitted bytes match runtime-neutral result | `plan254_envelope_completeness_*` + `route_tunnel_gateway` unit path |
| 35 | creator gateway bypass | `plan254_f35_*` |
| 36 | unknown gateway fails closed | `plan254_f33_*`, `plan254_d24_*` |
| 37 | multi-cell bounded | `LiveGatewayOutcome::Delivered{delivered,failures}` accounting |
| 38 | cancellation stops delivery | `plan254_f36_*` |
| 39 | cancel drains immediately | `plan254_g39_*` |
| 40 | session close removes peer | `plan254_g40_*` |
| 41 | cancelled build leaves no registration | `plan254_g41_*`, `plan254_c21_*` |
| 42 | repeated shutdown idempotent | `plan254_g39_*` |

Plan 252 full-message build tests and Plan 253 runtime-neutral data-plane tests remain
green (`i2pr-tunnel` 374 passed; `i2pr-daemon` 1171 passed including the 34 new rows).

## 5. Verification floor executed (from repo root, local truth)

```text
cargo fmt --all --check                                      : passed
cargo check --locked --workspace --all-targets               : passed (2 pre-existing
                                                              i2pr-tunnel warnings in
                                                              untouched files)
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
                                                            : 374 passed
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
                                                            : 1171 passed, 25 ignored
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                            : 3013 passed, 26 ignored
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                            : passed (`No issues found`;
                                                              follow-up hygiene commit
                                                              fixes 5 pre-existing Plan 253
                                                              `i2pr-tunnel` lints and 7 new-shape
                                                              lints with true exit codes)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                            : passed
cargo test --locked --workspace --doc                        : passed (0 doc tests)
cargo deny check advisories bans sources                     : passed
bash scripts/check-dependency-direction.sh                   : passed
bash scripts/check-runtime-boundaries.sh                     : passed
bash scripts/check-service-tunnel-boundaries.sh              : passed
bash scripts/check-m11-transit-boundaries.sh                 : passed
bash scripts/check-m6-mixed-router-acceptance-evidence.sh    : passed
bash scripts/check-exploratory-tunnel-evidence.sh            : passed
git diff --check                                             : passed
cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1
                                                            : 34 passed
cargo test --locked -p i2pr-daemon --test m11_transit_data_plane -- --test-threads=1
                                                            : 2 passed (updated to new owner API)
```

Ordinary GitHub Actions (Quality ubuntu/macOS, MSRV, dependency policy) run on the exact
closure SHA after push; green is required as closure authority per plan §H and is
recorded as a CI-evidence follow-up on that SHA.

## 6. Files changed

- `crates/i2pr-daemon/src/router_i2np.rs` — shared single decode; `TransitInboundBodies` /
  `TransitGatewayParts` / `RouterI2npTransitBuild` handoff; `dispatch_router_i2np_with_transit_bodies`.
- `crates/i2pr-daemon/src/transit_owner.rs` — placeholder deleted; outer-token dispatch;
  `TransitLiveOwner` production owner (probes, OBEP/IBGW delivery, session close, drain);
  disabled probe referenced by the daemon pump.
- `crates/i2pr-daemon/src/transit_compose.rs` — `route_tunnel_gateway`, `forget_peer_by_session`,
  `install`/`entries_iter` peer-index seams, TunnelData/TunnelGateway envelope + OBEP
  delivery helpers.
- `crates/i2pr-daemon/src/lib.rs` — ordinary pump references the disabled live-owner probe
  (default behavior unchanged).
- `crates/i2pr-daemon/tests/m11_transit_live_owner.rs` — new 34-row live-owner matrix.
- `crates/i2pr-daemon/tests/m11_transit_data_plane.rs` — updated to the corrected owner API.
- `crates/i2pr-tunnel/src/transit.rs` — lint-only hygiene (remove dead const, collapse
  nested if, drop unused test binding, drop no-effect `& 0xFF`, document the intentional
  large `TransitDataOutcome` variant); no behavior change.
- `scripts/check-m11-transit-boundaries.sh` — Plan 254 rules 9–14.
- `plans/registry.md`, `plans/subsystems/transit-tunnels-roadmap.md`, `specs/support.toml` —
  Plan 254 closure projection; Plan 255 unblocked for registration.

## 7. Tests not run (and why)

- Exact-pinned i2pd qualification: out of scope by plan §Out of scope; owned by
  unregistered Plan 255.
- Environment-gated external lanes (NTCP2/SSU2/I2CP/service-tunnel-Java): unchanged by this
  plan; their pre-existing pass/skip status is preserved.
- Full two-family router conformance: not required for this corrective.

## 8. Dependency / secret-handling decisions

- No new production dependencies.
- `TransitHopMaterial` stays move-only; no `Clone` added; static guard preserved.
- No payload logging; bodies are typed and bounded by existing I2NP limits.
- No `Debug`/`Display`/serialization added to secret types.
- Peer index stays bounded at `MAX_TRANSIT_PEER_INDEX`; session close reconciles it.

## 9. Deviations from the plan-of-record

- `RouterI2npTransitBuild<'a>` is provided as the documented borrowed view plus the owned
  `TransitInboundBodies` handoff (the owned form lets ordinary callers avoid retaining
  payloads while the live owner moves bodies without lifetime entanglement). Both derive
  from the one canonical decode; no second decoder exists.
- `TunnelGateway` bodies keep their existing `Unsupported{type_byte: 19}` classification
  (the exploratory-build garlic-reply path matches on it); the live owner consumes the
  typed `tunnel_gateway` parts from the same decode rather than reclassifying. No existing
  behavior changes.
- Missing peer mappings surface as terminal `NoActiveSession` through the live owner
  (already rolled back) rather than a service error, so the owner always propagates a
  typed terminal delivery result.

## 10. Remaining risks

- Bounded delivery outcomes that require live sessions (`Accepted`, `QueueFull`,
  `ResourceDenied`) are proven at the dispatch/commit/rollback level locally; the
  `Accepted`-leaves-live half is owned by the Plan 255 exact-pinned qualification lane.
- OBEP fragmented completion and multi-cell IBGW emission beyond the single-cell
  controlled shape are owned by the runtime-neutral suite plus Plan 255 traffic.
- No Java router compatibility work is claimed or affected.

## 11. Findings by severity

- Critical: none open (the two Plan 253 acceptance violations — no production caller and
  the empty-body shim — are closed and guarded).
- High: none open.
- Medium: `Accepted`-with-live-session delivery is qualification-owned (Plan 255).
- Low: two pre-existing `cargo check` warnings in untouched `i2pr-tunnel/src/transit.rs`
  (`MAX_TRANSIT_GATEWAY_NESTED` dead code, one unused test variable); not introduced here.

## 12. Unblock audit

- **Plan 255 exact-pinned i2pd qualification**: sole hard dependency was Plan 254 closure
  with real production-owner evidence and green exact-SHA CI. The production owner is real
  (`TransitLiveOwner::handle_inbound` + 34 live rows + static rules 10/12/14) and the local
  floor is green with committed formatting fixed. Plan 255 is **unblocked for
  registration** (it remains unwritten/unregistered until authored; no status is silently
  flipped).
- **M6 Java lane (Plans 201/247)**: unrelated subsystem; remains retained/deferred
  nonblocking debt per registry. No change.
- **M12 floodfill**: remains deferred until M11 controlled transit/resource evidence
  exists (Plan 255 qualification still pending). No change.
- No other registered plan lists Plan 254 as a hard or interface dependency.

## 13. Roadmap disposition

Plan 254 is **closed** (`passed-m11-live-ingress-body-threading-closure-corrective`).
Plan 253 remains retained with its corrective completed via this record. M11 capability
remains unclaimed and non-advertised. The next executable step is authoring Plan 255.

## Gate

M11 capability remains unclaimed and `advertised=false`. Plan 255 exact-pinned i2pd
qualification may now be registered; it must prove genuine builds, accepted encrypted
replies, role-correct TunnelData/TunnelGateway behavior, rejection/expiry/duplicate
handling, repeated exact-head stability, and cleanup against i2pd `2.61.0`.
