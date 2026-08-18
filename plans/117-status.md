# Plan 117 status — exploratory NetDB composition (Phases A–F landed)

- Status: **phases-a-through-f-landed**
- Date: 2026-08-18
- Plan of record: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- Predecessor: [`116-status.md`](116-status.md) — **passed-final-local-closure**
- Roadmap: [`115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md)
- Handoff: [`117-handoff.md`](117-handoff.md)
- Implementation floor commit: `0b2a487 tunnel: add Plan 117 Phase A-F outbound lookup and inbound dispatch`

## Current state

Plan 117 Phases A through F are implemented and locally green.

- **Phase A — typed NetDB action messages.** `LookupAction::SendDatabaselookup`
  carries the typed `DatabaseLookupMessage` it intends to dispatch, and
  `PublicationAttemptRecord` carries the typed `DatabaseStoreMessage` it
  intends to publish. `RouterInfoLookup` exposes
  `handle_pending_after_path(store, routing_key)` so the daemon seam can
  advance the state machine immediately after `accept_reply_path`
  succeeds.

- **Phase B — one-shot activation seam and role registry.** The new
  `crates/i2pr-tunnel::data_plane_registry` module owns the typed
  `DataPlaneCapacity`, `DataPlaneRegistry`, and `RegistryError` types.
  `ExploratoryPool::activate(slot)` returns the typed `EstablishedTunnel`
  for the runtime scheduler; the registry retains the activated
  `OutboundGatewayRole` and `LocalInboundEndpointRole` plus their public
  first-hop metadata.

- **Phase C — daemon NetDB seam composition root.** `NetDbSeam` no
  longer stops at `accept_reply_path`. After the reply path is bound,
  the seam drives `RouterInfoLookup::handle_pending_after_path` and
  emits the next typed `SendDatabaselookup` action. The seam now
  exposes the bounded `CompositionOutcome` vocabulary
  (`NeedInboundExploratory`, `NeedOutboundExploratory`,
  `LookupReadyForTunnelDispatch`, `NoEligibleCandidates`) so the
  daemon scheduler can request the right build at every step.

- **Phase D — outbound `DatabaseLookup` through the real outbound
  role.** The new `crates/i2pr-daemon::outbound_lookup` module owns
  `encode_standard_envelope`, `compose_outbound_lookup`, and
  `OutboundLookupDispatch`. The composition root drives the existing
  `OutboundGatewayRole::forward_cells` against a `Router`-delivery
  `TunnelPayloadHeader` and packages the resulting TunnelData cells
  as `i2pr_transport::DeliveryRequest`s addressed to the outbound
  first hop. The dispatch never hand-builds an STBM or a standard
  I2NP envelope; it uses the existing `i2pr-proto` standard encoder.

- **Phase E — inbound exploratory `TunnelData` dispatch.** The new
  `crates/i2pr-daemon::inbound_dispatch` module owns
  `dispatch_inbound_tunnel_data`, `route_databasestore`, and
  `route_database_search_reply`. The dispatch helper routes cells by
  `tunnel_id` against the activated `LocalInboundEndpointRole`,
  decodes the recovered standard I2NP envelope exactly once through
  `i2pr-proto`, and supports only `DatabaseStore`,
  `DatabaseSearchReply`, and `DeliveryStatus` body kinds. Unknown
  tunnel identifiers fail closed without allocating role state.

- **Phase F — local RouterInfo publication through the
  exploratory path.** `PublicationAttemptRecord` retains the typed
  `DatabaseStoreMessage`, and `outbound_lookup::compose_outbound_publication`
  routes the publication through the same outbound exploratory data
  plane, with `ROUTER` delivery to the selected floodfill Router Hash.

## Validation

Focused checks executed at the implementation floor commit
(`0b2a487`):

```text
cargo +1.95.0 fmt --all --check            # clean
cargo +1.95.0 check --locked --workspace --all-targets  # clean
cargo +1.95.0 test --locked --workspace    # 721 passed (35 suites)
cargo +1.95.0 test --locked -p i2pr-daemon --all-targets  # 92 passed
```

A pre-existing clippy `bool_comparison` warning at
`crates/i2pr-tunnel/src/pool.rs:626` predates Plan 117 and is
documented as a known pre-existing clippy caveat on this host.
Plan 117 does not introduce new clippy failures.

## What remains

- **Phase G** — mandatory all-i2pr deterministic production-seam
  integration test that exercises the full composition through real
  `EstablishedMaterial` derived from successful short-build paths.
  The construction of a real `EstablishedMaterial` from a successful
  build state machine requires the Plan 116 test-only helpers and
  is sequential work deferred to a follow-up commit.
- **Phase H** — pinned Emissary
  (`9b43484a21d5a1291c4881cdae62a36c527f8c0f`) native mixed-router
  checkpoint (`tests/integration/ntcp2/harness/test_pinned_native_mixed_router_emissary_checkpoint.rs`).
  This lane requires either the Plan 046 rootless sealed-namespace
  lane or the Plan 048/049 Multipass recovery lane. Both are
  blocked on this host (`blocked_execution_lane_unavailable`).
- **Phase I** — inspect an existing authenticated transport lane to
  confirm that no NTCP2/SSU2 helper is required for the local
  composition. The current `tools/i2pr-interop` surface remains
  `non-production` and non-advertised.
- **Phase J** — propagate the Plan 117 status into `README.md`,
  `AGENTS.md`, the Plan 117 plan file, the i2pr-ntcp2-interop skill
  (no impact: the skill tracks NTCP2 evidence, not Plan 117), and
  the architecture documents (`docs/architecture/{overview,i2pr-netdb,i2pr-daemon}.md`).
  This document is the first half of Phase J; the docs propagation
  lands in the same commit as Phase G/H/I closure.

## Risk and follow-up

- Phase G test design should use the canonical `TunnelEntry` /
  `EstablishedTunnel` pools that Phase 116 closed so that the
  integration test does not allocate placeholder state.
- The Emissary checkpoint (Phase H) requires either a qualified
  rootless namespace or the Multipass recovery lane. Plan 047
  defers cross-host portability to a follow-up plan.
- A future pinned Java revision that exposes a transport-only
  direct seam may re-issue ADR 0021, but does not affect the Plan 117
  composition surface.

## Authority sources

- Plan file: `plans/117-live-exploratory-netdb-integration.md`
- Plan 116 closure: `plans/116-status.md`
- Handoff: `plans/117-handoff.md`
- Plan 115 canonical bridge: `crates/i2pr-tunnel/src/bridge.rs`
- Composition helpers: `crates/i2pr-daemon/src/{outbound_lookup,inbound_dispatch}.rs`
- Data-plane registry: `crates/i2pr-tunnel/src/data_plane_registry.rs`
