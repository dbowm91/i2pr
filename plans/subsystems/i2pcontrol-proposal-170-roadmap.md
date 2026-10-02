# Proposal 170 / I2PControl Parallel Roadmap

Status: Plans 286–289 passed; Plans 290, 291, and 294 ready; Plans 292–293 and 295 registered behind the dependency graph. This workstream is parallel to M12 and does not gate router-mainline progression.

Long-term references:
- GUARDRAILS.md
- specs/CONFORMANCE.md
- specs/support.toml
- docs/adr/0001-modular-monolith.md
- docs/adr/0002-tokio-runtime-boundary.md
- docs/adr/0028-i2pcontrol-proposal-170-control-plane.md
- plans/subsystems/service-tunnels-roadmap.md
- plans/subsystems/floodfill-roadmap.md

External contract/reference pins at roadmap creation:
- I2P Proposal 170, I2PControl Expansion, Open, revision 2026-05-20.
- Base I2PControl documentation, API version 1 / JSON-RPC 2.0 semantics, site documentation updated 2026-07-10.
- eggstack/emissary fork master 6885a945d25a5ae61bc68191d27c5816bc3df4c9.
- eepnet/emissary upstream master 9b43484a21d5a1291c4881cdae62a36c527f8c0f; no i2pcontrol paths in the researched upstream tree.
- Java I2PControl Proposal 170 PR 6 head 45bb593000408071dd376b78848fdc246dccd964.
- PurpleI2P/i2pd openssl head 2d57d3f6783efbfebde6c5b03f29e6c231a84d6b for adopted/base I2PControl behavior.

## 1. Purpose and ownership boundary

This workstream implements Proposal 170 as i2pr's standard administrative/control plane so external applications and future frontends can manage the router through a router-neutral contract.

It does not make I2PControl a router core. i2pr-i2pcontrol owns runtime-neutral protocol/domain semantics. i2pr-daemon owns listener/TLS/authentication and composition. Existing router subsystems remain authoritative for the state they expose. A new i2pr-addressbook subsystem becomes the canonical naming/address-book owner rather than control-plane shadow state.

## 2. Work classification

Plan 286 is invariant/architecture/provenance foundation.

Plan 287 is infrastructure plus the minimum secure base I2PControl capability.

Plans 288, 289, and 294 are independently executable capability branches after Plan 287:
- read-only RouterInfo and ClientServicesInfo;
- TunnelManager control/lifecycle over the existing M10 owner;
- canonical AddressBook and resolver integration.

Plans 290–293 complete TunnelManager families and options. Plan 291 is intentionally separate because Streamr requires a new repliable-datagram substrate.

Plan 295 is the integrated conformance/security/restart/differential closure.

## 3. Non-goals

This roadmap does not build a frontend, router console, web UI, TUI, desktop UI, or frontend-specific state store.

It does not implement unrelated base I2PControl methods merely for completeness. GetRate, RouterManager, NetworkSetting, AdvancedSettings, or other non-Prop-170 methods require their own scope unless a base-compatibility test requires an explicit method-not-found response.

It does not create a second NetDB, destination runtime, tunnel manager, SAM bridge, I2CP server, logging subsystem, or unrestricted global router context.

It does not alter M12 dependency ordering or capability advertisement.

## 4. Current state

At registration:
- no I2PControl listener or JSON-RPC administrative service exists in i2pr;
- SAM and I2CP are closed local application-protocol products;
- M10 ServiceTunnelManager owns six service kinds: generic client/server, HTTP client, SOCKS5 client, IRC client, IRC server;
- Proposal 170 defines twelve I2PTunnel-family types, so composed HTTP/CONNECT/SOCKS-IRC server profiles and Streamr remain;
- i2pr has no repliable-datagram application substrate;
- no canonical AddressBook subsystem is present;
- RouterInfo/ClientServices data is spread across truthful existing owners but lacks one bounded administrative inspection contract;
- DestinationIdentity currently enforces the workspace signing type for router-owned/client-owned destination parsing, so dynamic SigType is not a superficial TunnelManager option;
- LeaseSet security/client-auth options require real blinded/encrypted LeaseSet2 and lookup ownership, not inert persistence.

The eggstack/emissary fork contains a mature fork-specific Proposal 170 implementation and conformance history. It is a donor/reference for protocol/domain/security/test work, not an architecture to transplant wholesale.

## 5. Target architecture

~~~text
future frontend / external administrator
              |
          I2PControl
              |
      HTTPS + JSON-RPC 2.0
              |
      i2pr-i2pcontrol
  exact bounded wire/domain contract
              |
      i2pr-daemon adapters
   /       /        |        \
  /       /         |         \
identity NetDB  service mgr  addressbook
runtime  peers   destinations   owner
tunnels reach.  SAM/I2CP
~~~

All control consumers see point-in-time bounded snapshots or typed commands. They never receive mutable subsystem internals.

## 6. Dependency graph

~~~text
286 authority/provenance/contract foundation
  -> 287 secure base I2PControl server
       -> 288 RouterInfo + ClientServicesInfo
       -> 289 TunnelManager control/lifecycle
            -> 290 composed missing stream families
            -> 291 repliable datagram + Streamr
                 \ /
                 292 exact noncrypto option completion
                   -> 293 deep signature/LeaseSet/provider options
       -> 294 canonical AddressBook + resolver integration

288 + 293 + 294
  -> 295 full source completion + integrated differential conformance
~~~

Plans 288, 289, and 294 may execute concurrently once Plan 287 is closed. Plans 290 and 291 may execute concurrently once Plan 289 is closed.

## 7. Milestones

| Plan | State | Classification | Handoff | Closure |
|---|---|---|---|---|
| 286 | passed | invariant/infrastructure | plans/implementation/i2pcontrol-proposal-170/286-parallel-authority-provenance-and-contract-foundation.md | plans/closure/i2pcontrol-proposal-170/286-status.md (`passed-prop170-parallel-authority-provenance-and-contract-foundation`) |
| 287 | passed | infrastructure/capability | plans/implementation/i2pcontrol-proposal-170/287-secure-base-i2pcontrol-jsonrpc-auth-tls.md | plans/closure/i2pcontrol-proposal-170/287-status.md (`passed-prop170-secure-base-i2pcontrol-jsonrpc-auth-tls`) |
| 288 | passed | capability | plans/implementation/i2pcontrol-proposal-170/288-routerinfo-and-clientservices-inspection-plane.md | plans/closure/i2pcontrol-proposal-170/288-status.md (`passed-prop170-routerinfo-and-clientservices-inspection-plane`) |
| 289 | passed | capability/infrastructure | plans/implementation/i2pcontrol-proposal-170/289-tunnelmanager-control-state-and-existing-service-adapter.md | plans/closure/i2pcontrol-proposal-170/289-status.md (`passed-prop170-tunnelmanager-control-state-and-existing-service-adapter`) |
| 290 | ready | capability | plans/implementation/i2pcontrol-proposal-170/290-composed-tunnel-family-parity.md | future |
| 291 | ready | capability/infrastructure | plans/implementation/i2pcontrol-proposal-170/291-repliable-datagram-and-streamr-tunnel-families.md | future |
| 292 | blocked on 290 + 291 | capability | plans/implementation/i2pcontrol-proposal-170/292-tunnel-option-matrix-and-noncrypto-runtime-completion.md | future |
| 293 | blocked on 292 | capability/crypto integration | plans/implementation/i2pcontrol-proposal-170/293-signature-leaseset-security-and-provider-option-completion.md | future |
| 294 | ready | capability | plans/implementation/i2pcontrol-proposal-170/294-canonical-addressbook-and-resolver-integration.md | future |
| 295 | blocked on 288 + 293 + 294 | evidence/closure | plans/implementation/i2pcontrol-proposal-170/295-full-source-completion-and-cross-router-conformance.md | future |

## 8. Cross-cutting requirements

- I2PControl is disabled by default and loopback-only by default.
- No protected method dispatch occurs before successful Authenticate.
- Every body, batch, string, collection, token table, throttle table, request, connection, task, timer, and response has an explicit ceiling.
- Non-loopback exposure fails closed without explicit operator-owned TLS material.
- No plaintext fallback.
- Passwords, tokens, destination private material, LeaseSet secrets, proxy passwords, client-auth secrets, and raw sensitive configuration are never logged or reflected.
- Unsupported/missing capability is explicit. Never fabricate zero, false, empty, or nearby state.
- Startup TOML definitions and I2PControl-owned definitions retain provenance and cannot silently overwrite one another.
- A persisted option counts as supported only if a real owner applies it.
- AddressBook administrative views and normal .i2p resolution share one owner.
- The workstream remains frontend-neutral.
- Mainline router failure must not be caused by an optional disabled I2PControl feature.

## 9. Verification strategy

Every plan uses the routine workspace floor plus focused contract tests.

The control protocol requires literal request/response fixtures, malformed JSON/JSON-RPC cases, duplicate/unknown field cases, size +1 cases, notification and batch behavior, authentication/error vectors, cancellation and timeout tests, and feature-disabled/no-side-effect tests.

Mutation plans require deterministic rollback/restart/failure-injection evidence. Secret-bearing fields require redaction tests.

Final differential evidence uses:
- eggstack/emissary as the project-owned mature Proposal 170 behavioral/conformance oracle;
- Java PR 6 for the proposed Java wire/administrative semantics;
- i2pd for the adopted base/RouterInfo/ClientServicesInfo semantics it actually implements.

These are contract comparisons, not router-to-router anonymity/conformance claims.

## 10. Risks and decision points

Primary risks are creating a shadow router state, claiming support from parser acceptance, allowing remote administrative exposure to outrun authentication/TLS policy, persisting secrets unsafely, and expanding TunnelManager into a second service runtime.

The deepest technical risks are Streamr's missing datagram substrate, dynamic destination signature ownership, encrypted/blinded LeaseSet2/client authorization, and outproxy-provider semantics.

Proposal 170 is still Open. The pinned 2026-05-20 contract is authoritative for this roadmap; later proposal changes require an explicit reconciliation plan before they silently change implemented wire behavior.

## 11. Completion definition

This workstream is complete only when:
- base I2PControl API-1 authentication and JSON-RPC behavior are externally usable through the secured optional listener;
- all Proposal 170 RouterInfo selectors have truthful typed sources or an explicitly protocol-permitted neutral disposition;
- ClientServicesInfo reports actual bounded service state;
- AddressBook mutations and getters use the same canonical owner as ordinary resolution;
- all twelve TunnelManager types have real lifecycle backends;
- every applicable TunnelManager option has a real effect or a spec-justified explicit non-applicability; no inert accepted options remain;
- mutations are atomic/recoverable across restart and failures;
- disabled/default mode is isolated;
- integrated security, resource, and differential conformance evidence passes;
- specs/support.toml and user-facing docs describe exactly the proven subset;
- no frontend claim or implementation is bundled into closure.

## 12. Status summary

Plan 286 passed with the frozen provenance manifest, the runtime-neutral
`i2pr-i2pcontrol` contract crate, and the conformance dimensions (see
`plans/closure/i2pcontrol-proposal-170/286-status.md`). Plan 287 passed
with the secured optional loopback-default listener, API-1 authentication,
and the typed dispatch floor (see
`plans/closure/i2pcontrol-proposal-170/287-status.md`). Plan 288 passed
with the truthful bounded source matrix (5 live + 16 publish-gated + 9
unavailable, 0 permitted-neutral), the narrow daemon inspection handles,
and the RouterInfo/ClientServicesInfo select-form dispatch with
whole-request gap failures (see
`plans/closure/i2pcontrol-proposal-170/288-status.md`); residual NetDB,
transport, tunnel, rate, log, news, and ban sources belong to Plan 295
(publication wiring) and Plan 294 (AddressBook). Plan 289 passed with
the seven-action TunnelManager control plane over the one existing M10
manager for the six families with real backends, the versioned
generation store, and the validate-mirror-stage-reconcile-publish-verify
transaction (see
`plans/closure/i2pcontrol-proposal-170/289-status.md`); the remaining
six types belong to Plans 290–291 and the wider option matrix to Plans
292–293. Plans 290, 291, and 294 are ready.
The remainder of the workstream is registered but dependency-gated. No
Proposal 170 capability is claimed by registration alone.
