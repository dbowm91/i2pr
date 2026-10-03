# Plan 290 — Composed Proposal 170 tunnel-family parity

Status: registered-prop170-composed-tunnel-families-blocked-on-plan289

Classification: capability.

Hard dependency: Plan 289 closed.

## Objective

Add the non-Streamr Proposal 170 tunnel families that are absent as first-class i2pr M10 service kinds by composing existing streaming, HTTP, SOCKS, IRC, destination, and service-manager primitives.

This plan does not add repliable datagrams; Plan 291 owns streamrclient/streamrserver.

## Required families

### connectclient

Add a dedicated strict HTTP CONNECT client profile using the existing HTTP target validation, I2P destination resolution, listener/auth policy, and streaming byte pump. It is not merely an alias in the wire model: the service kind must have its own option applicability and lifecycle identity while reusing shared primitives internally.

### socksirc

Compose SOCKS negotiation/target selection with the IRC client privacy filter. After CONNECT succeeds, all application traffic must pass through the IRC filter; there is no raw bypass mode.

Reconcile Proposal/reference expectations for SOCKS4a/SOCKS5. i2pr M10 currently documents SOCKS5 only. If the pinned Proposal/Java contract expects the historical I2PTunnel SOCKS profile to accept SOCKS4a, implement bounded SOCKS4a CONNECT in the shared parser and preserve the existing SOCKS5 behavior. Literal IPv4/IPv6/direct clearnet and unsupported commands remain fail-closed unless explicitly required by the pinned contract.

### httpserver

Add a server profile over the existing persistent server destination + accepted streaming connection path. Introduce runtime-neutral inbound HTTP parsing/filtering required for the I2PTunnel server privacy contract. The local target remains bounded/loopback-controlled by the service policy. Do not reuse the client proxy parser in a way that conflates request-target semantics.

### httpbidirserver

Implement the deprecated bidirectional HTTP server as a composition of the filtered HTTP server half and a local HTTP client/proxy half under one lifecycle generation and one persistent public server identity. It must not create a second independently published server destination.

Deprecation does not permit unsafe shortcut semantics. Unsupported clearnet/outproxy behavior remains explicit.

## ServiceTunnelKind integration

Extend the runtime-neutral service-tunnel kind enum, config validation, diff classification, snapshots, manager backend dispatch, and startup/reconcile tests.

No new family may bypass:
- aggregate/per-service connection permits;
- destination ownership;
- generation/draining lifecycle;
- local target/bind validation;
- router-delivery handle;
- secret/log policy.

## Shared policy extraction

Where Emissary's Prop 170 fork has mature filters or option contracts, reuse only manifest-authorized Prop 170 code and translate it to i2pr's runtime-neutral crates. Do not import Yosemite sessions, Emissary supervisors, or unrelated proxy code.

Prefer extracting shared i2pr primitives so existing M10 profiles and new Proposal profiles use one parser/filter/pump implementation rather than forks.

## Evidence

For each new family:
- config parse/validation and exact Proposal mapping;
- listener/start/stop/restart/delete through TunnelManager;
- local deterministic positive data path;
- malformed/slow/oversized input;
- sibling isolation and aggregate resource ceilings;
- cancellation/half-close/target failure cleanup;
- generation replacement/draining;
- no destination identity churn on no-op or target-only transitions where identity should persist;
- no direct-clearnet escape;
- privacy-filter regression cases.

Run the existing M10 suite unchanged to prove no regression of current service kinds.

## Acceptance criteria

Plan 290 closes when connectclient, socksirc, httpserver, and httpbidirserver are real bounded backends under the existing ServiceTunnelManager, the ordinary socks profile has the required pinned protocol parity, and all lifecycle actions from Plan 289 work on them.

At closure, ten of the twelve Proposal tunnel families must have real backends. The two Streamr families remain exclusively Plan 291 scope.


## ADR 0030 anonymity boundary note

Every new tunnel family in this plan composes over the canonical Destination-group owner from anonymity Plans 309/310. Multiple server families may intentionally share one persistent Destination when their I2P inbound ports are distinct. Multiple client families may intentionally share a client Destination group.

HTTP server and bidirectional HTTP server defaults must not inject hosting-router identity, RouterInfo hash/version, transport address, local hostname/path, or i2pr build identity into backend-facing headers. Any backend header identifying the remote client Destination is opt-in, bounded, spoof-resistant, and clearly distinct from router identity. Plan 307's service-boundary checker/invariants apply to these future families.
