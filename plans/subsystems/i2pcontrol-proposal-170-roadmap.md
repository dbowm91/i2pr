# Proposal 170 / I2PControl Parallel Roadmap

Status: `qualified-profile-closed` for the historical i2pr profile in Plans 286–297. Full Proposal 170 conformance is not claimed. Plans 319–321 and 323–324 passed. Plans 322 and 327 are closed blocked on missing production owners; Plan 325 is blocked after a completed provider survey; Plan 326 is closed blocked only on Plan 325; Plan 328 is closed blocked on 322, 326, and 327. No active Proposal 170 continuation plan remains in this line. This workstream remains parallel to M12/mainline.

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
| 290 | passed | capability | plans/implementation/i2pcontrol-proposal-170/290-composed-tunnel-family-parity.md | plans/closure/i2pcontrol-proposal-170/290-status.md (`passed-prop170-composed-tunnel-families`) |
| 291 | passed | capability/infrastructure | plans/implementation/i2pcontrol-proposal-170/291-repliable-datagram-and-streamr-tunnel-families.md | plans/closure/i2pcontrol-proposal-170/291-status.md (`passed-prop170-repliable-datagram-and-streamr`) |
| 292 | passed | capability | plans/implementation/i2pcontrol-proposal-170/292-tunnel-option-matrix-and-noncrypto-runtime-completion.md | plans/closure/i2pcontrol-proposal-170/292-status.md (`passed-prop170-tunnel-option-matrix-and-noncrypto-completion`) |
| 293 | passed | capability/crypto integration | plans/implementation/i2pcontrol-proposal-170/293-signature-leaseset-security-and-provider-option-completion.md | plans/closure/i2pcontrol-proposal-170/293-status.md (`passed-prop170-deep-tunnel-option-determinations`) |
| 294 | passed | capability | plans/implementation/i2pcontrol-proposal-170/294-canonical-addressbook-and-resolver-integration.md | plans/closure/i2pcontrol-proposal-170/294-status.md (`passed-prop170-canonical-addressbook-and-resolver-integration`) |
| 295 | passed | evidence/closure | plans/implementation/i2pcontrol-proposal-170/295-full-source-completion-and-cross-router-conformance.md | plans/closure/i2pcontrol-proposal-170/295-status.md (`passed-prop170-full-source-completion-and-differential-conformance`) |
| Proposal 170/296 | passed | evidence/closure | plans/implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md | plans/closure/i2pcontrol-proposal-170/296-status.md (`passed-prop170-pool-shaping-and-bundling-residuals`) |
| Proposal 170/297 | passed | evidence/closure | plans/implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md | plans/closure/i2pcontrol-proposal-170/297-status.md (`passed-prop170-local-tls-identity-for-use-ssl`) |

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
292–293. Plans 290–297 closed the historical qualified profile. Subsequent audit found that the public contract did not match the exact Proposal 170 wire contract and that external differential evidence was not executed. Plans 319–320 close planning reconciliation and canonical-wire migration; Plans 321–328 own remaining operational capabilities and the final live conformance gate. No new capability is claimed by registration alone.


## 13. Full Proposal 170 conformance continuation (Plans 319–328)

### Claim vocabulary

- **`qualified-profile-closed`** — the Plans 286–297 experimental profile and its explicit incompatibilities are closed as implemented.
- **`canonical-wire`** — exact Proposal 170 names, parameter shapes, return types, and action semantics.
- **`full-proposal-conformant`** — canonical wire plus every Proposal-required capability is operational or explicitly implementation-dependent under the Proposal, with live external evidence.

The historical phrase “workstream is fully closed” in the Proposal 170/297 closure is scoped to `qualified-profile-closed`; it does not establish `full-proposal-conformant`. Historical status files remain unchanged. The subsystem-qualified collision identities and interpretation are recorded in [`plans/global-number-collision-ledger.md`](../global-number-collision-ledger.md).

Post-297 audit identified four classes that prevent an unqualified full Proposal 170 claim:

1. Planning authority: historical global plan-number collisions at 296/297, duplicate registry rows, and stale “fully closed” wording.
2. Canonical wire: the current control crate exposes normalized RouterInfo/TunnelManager/AddressBook vocabulary rather than the exact Proposal 170 public names and shapes.
3. Capability gaps: live AddressBook subscription fetching/publishing, signed router news and exact RouterInfo shapes, destination key-policy semantics, encrypted/blinded LeaseSet2 client authorization, and I2P-routed outproxy-provider behavior.
4. Evidence: Plan 295’s retained external differential row is blocked-env-absent; full conformance requires executed reference lanes.

Current continuation graph:

    319 planning authority / numbering reconciliation
      -> 320 canonical Proposal 170 wire contract
           -> 321 AddressBook operational completion
                -> 322 canonical RouterInfo + signed news
           -> 323 canonical TunnelManager non-deep parity
                -> 324 destination SigType/EncType policy
                -> 327 I2P-routed outproxy provider
           -> 325 Red25519 provider qualification
                -> 326 encrypted/blinded LS2 + client auth
                     (also requires 323 + 324)

    322 + 326 + 327 -> 328 live external full-conformance gate

Plans 321, 323, and 325 were independently eligible after Plan 320. Plan 325's provider survey closed blocked: no reviewed packaged Rust provider matched I2P Red25519. Plans 321, 323, and 324 passed; Plans 322, 326, and 327 closed blocked on their recorded missing owners/dependencies; Plan 328 closed blocked because its required plans could not pass. Plans 329–335 now supersede only the forward Red25519/Encrypted-LeaseSet architecture: Plan 329 established a clean-room/spec-first path over maintained curve primitives, Java/i2pd readable references, and Emissary black-box-only differential, and Plans 330–333 have since passed. Plan 334 is blocked with its control plane complete but its blocker upstream of itself: a tunnel created through TunnelManager is reconciled onto a manager instance that is not the product layer's and that carries no router delivery capability, so it publishes no LeaseSet2 at all. That also contradicts this roadmap's Plan 289 "one existing M10 ServiceTunnelManager" invariant, and the drift is recorded rather than corrected in place. Plan 335 is blocked behind it. The unqualified full Proposal 170 claim is still not made.

| Plan | State | Classification | Handoff | Closure |
|---|---|---|---|---|
| 319 | passed | planning invariant/tooling corrective | plans/implementation/i2pcontrol-proposal-170/319-planning-authority-and-global-number-reconciliation.md | plans/closure/i2pcontrol-proposal-170/319-status.md (`passed-prop170-planning-authority-and-global-number-reconciliation`) |
| 320 | passed | protocol contract corrective | plans/implementation/i2pcontrol-proposal-170/320-canonical-wire-contract-reconciliation.md | plans/closure/i2pcontrol-proposal-170/320-status.md (`passed-prop170-canonical-wire-contract-reconciliation`) |
| 321 | passed | capability/I-O composition | plans/implementation/i2pcontrol-proposal-170/321-addressbook-operational-completion.md | plans/closure/i2pcontrol-proposal-170/321-status.md (`passed-prop170-addressbook-operational-completion`) |
| 322 | blocked | capability/observability/signed content | plans/implementation/i2pcontrol-proposal-170/322-routerinfo-canonical-source-and-news-completion.md | plans/closure/i2pcontrol-proposal-170/322-status.md (`blocked-prop170-production-transit-and-ipv6-source-owners`) |
| 323 | passed | capability/protocol parity | plans/implementation/i2pcontrol-proposal-170/323-tunnelmanager-canonical-nondeep-parity.md | plans/closure/i2pcontrol-proposal-170/323-status.md (`passed-prop170-tunnelmanager-canonical-nondeep-parity`) |
| 324 | passed | crypto integration/identity lifecycle | plans/implementation/i2pcontrol-proposal-170/324-destination-signing-and-encryption-policy.md | plans/closure/i2pcontrol-proposal-170/324-status.md (`passed-prop170-destination-signing-and-encryption-policy`) |
| 325 | blocked | crypto provider qualification | plans/implementation/i2pcontrol-proposal-170/325-red25519-provider-qualification.md | plans/closure/i2pcontrol-proposal-170/325-status.md (`blocked-no-qualified-maintained-i2p-red25519-provider`) |
| 326 | blocked | encrypted LeaseSet capability | plans/implementation/i2pcontrol-proposal-170/326-encrypted-leaseset-and-client-authorization.md | plans/closure/i2pcontrol-proposal-170/326-status.md (`blocked-prop170-encrypted-leaseset-awaiting-qualified-red25519-provider`) |
| 327 | blocked | I2P-routed outproxy capability | plans/implementation/i2pcontrol-proposal-170/327-i2p-routed-outproxy-provider.md | plans/closure/i2pcontrol-proposal-170/327-status.md (`blocked-prop170-outproxy-provider-needs-routed-provider-and-secret-owner`) |
| 328 | blocked | live external conformance gate | plans/implementation/i2pcontrol-proposal-170/328-live-external-full-conformance-gate.md | plans/closure/i2pcontrol-proposal-170/328-status.md (`blocked-prop170-full-conformance-gate-awaiting-322-326-327`) |

The term full-proposal-conformant is reserved for a passing Plan 328 against its re-frozen Proposal revision. The 286–297 closure remains qualified-profile-closed and is not relabeled.


## 14. Red25519 / Encrypted LeaseSet2 clean-room successor (Plans 329–335)

Detailed authority: `plans/subsystems/red25519-encrypted-leaseset-roadmap.md`.

Historical Plans 325 and 326 remain closed blocked and are not rewritten. Their forward architecture
is superseded by Plans 329–335 because the project will independently implement the I2P-specific
Red25519 construction over maintained curve arithmetic rather than wait for a packaged Red25519
crate.

Reference boundary:
- I2P specifications are normative.
- Java I2P and i2pd may be read as interoperability/ambiguity references.
- Emissary Red25519/ELS2 source is excluded from direct reuse for this branch.
- Emissary may be used only after implementation freeze as a black-box differential oracle.

Current graph (`passed` / `ready` / `blocked`):

```text
329 passed  provenance/reference correction + spec freeze
  -> 330 passed  independent Red25519 implementation
       -> 331 passed  independent qualification
            -> 332 passed  type-5 ELS2 foundation
                 -> 333 passed  PSK/DH client auth
                      -> 334 passed  canonical Prop 170 mode mapping + control surface
                           -> 335 blocked  live ELS2 interoperability/reclosure
                                (closed blocked, then corrected twice:
                                 both references DO implement the domain,
                                 and BOTH lanes are now measured — i2pd
                                 and Java verify each other's type-11
                                 signatures and both reject i2pr's, because
                                 both implemented Zcash RedDSA instead of
                                 I2P Red25519. A measured negative result,
                                 not a missing build)

337 passed  control-owned service tunnels reach the product layer
  (corrective pass on Plan 289: two ServiceTunnelManager instances, the
  control-owned one never given a router delivery capability)
  -> 338 passed  one owner for the service identity store + transaction rollback
       (corrective pass on Plans 289 and 334, found while implementing 337;
        also corrects 337's own gap-1 diagnosis)

339 passed   per-family network status/error/testing owners
  (reopens Plan 322 for FIVE selectors only:
   i2p.router.net.status.v6 / .error / .error.v6 / .testing / .testing.v6.
   The Proposal marks all five "(adopted from i2pd)", so the
   enumeration is pinned to i2pd 2c69414 RouterContext.h:44-72
   rather than invented, and every code derives from reachability
   state i2pr already maintains. The three TRANSIT selectors stay
   Plan 322 Group A: that is a transit-participation posture
   change, not a missing snapshot.)

340 passed   transit volume / bandwidth / share owners
  (takes Plan 322 Group A's three selectors:
   net.total.transit.bytes / net.bw.transit.15s / net.tunnels.shareratio.
   Ownership only. The participation posture is NOT changed:
   TransitParticipation::Disabled is the enforced production state
   and holds no counters, so it cannot report non-zero volume.
   Honest production baseline is 0 / 0 / 0.0. shareratio is the one
   key the Proposal does not mark "(adopted from i2pd)" and that no
   pinned reference implements, so i2pr defines and labels it
   locally and fails closed without an attested denominator.)

341 passed   restart-safe non-echoing outbound proxy secret owner
  (the prerequisite Plan 327's closure named as its blocker.
   ProxyCredentials is a ONE-WAY inbound verifier and cannot
   produce the password an I2P-routed outproxy must send, so no
   provider is buildable first. Runtime-neutral capability trait in
   i2pr-service-tunnels + router-bound ChaCha20-Poly1305 in the
   daemon, keyed by HKDF over the persisted signing seed.
   No pinned reference is authority: pinned i2pd's outproxy is
   clearnet-only, and the Java at-rest scheme is unverified here.)

343 passed   I2P-routed outproxy provider policy + route owner
  (The provider Plan 327's record diagnosed as absent, in both
   halves: a runtime-neutral policy layer plus a daemon route owner.
   The no-direct-clearnet invariant is enforced STRUCTURALLY (an
   outproxy entry that is not an I2P destination is refused, so one
   cannot even name a non-I2P target), BEHAVIOURALLY (a clearnet
   target with no provider is a typed refusal, never a direct route),
   and STATICALLY (rules 9-11 of check-service-tunnel-boundaries.sh
   scan both files for socket/resolver/plugin spellings, with a
   positive control; all three inversions fail closed). The outproxy
   clearnet target is a SEPARATE grammar from http::target, so the
   direct path's .i2p-only enforcement is not weakened. Four defects
   were caught by its own rows, including SOCKS5 IPv4/IPv6 reply
   framing two bytes short. NOT reachable from any request path:
   infrastructure, not a capability.)

342 registered  outproxy option surface + request paths + wire lane
  (Plan 342 retains the remainder after Plan 343: ProxyList /
   UseOutproxyPlugin / OutproxyAuth / OutproxyUsername /
   OutproxyPassword / OutproxyType / SSLProxies semantics with
   transactional reconfiguration, HTTP / CONNECT request-path
   integration and the Proposal-applicable SOCKS families, and the
   self-composed in-tree loopback outproxy wire lane -- loopback
   evidence, NOT interoperability, per the Plan 339 decision.
   Plan 327 stays BLOCKED: the provider exists but no client can
   reach it.)

PLAN 322 GAP CENSUS: ZERO. Plan 322 was amended to passed on
2026-10-05; all 43 canonical RouterInfo additions now have a named
owner. The transit-participation POSTURE is unchanged and stays a
separate decision requiring M11 re-qualification bound to this tree.
```

Every obstacle found on this control path is now removed: one manager, a
publication path a control-created service reaches, a type-5 record at the
record's own blinded storage key, a resolving `.b32.i2p` **on the JSON-RPC
wire**, and transactions that leave nothing behind when they fail.

**Plan 322's eight gaps are two different problems, and the closure record
compressed them into one.** Re-auditing the source on 2026-10-05 split them:

- **Group A — the three transit selectors** (`net.total.transit.bytes`,
  `net.bw.transit.15s`, `net.tunnels.shareratio`). `TransitBuildService` and its
  bounded `TransitCounters` already exist; production profiles simply never
  construct the service, because transit participation is deliberately disabled.
  So this is a **production transit-participation posture change**, gated by M11
  qualification and the constrained-host lane — not the "add a bounded snapshot
  at the owning subsystem" that the Plan 322 text implies. It stays blocked and
  is not picked up here.
- **Group B — the five per-family selectors.** These project state i2pr already
  maintains: `ReachabilityState` (with `AddressFamily` and expiry), configured
  versus bound sockets, and the attested NetDB peer snapshot. Plan 339 takes
  Group B.

The decisive input was the Proposal text itself, re-retrieved read-only and
hash-verified in-repo: all five Group B selectors are marked **"(adopted from
i2pd)"**. The integer vocabulary is therefore i2pd's, pinned at `2c69414`
(`RouterContext.h:44-72`) rather than chosen by i2pr, and the *conditions* under
which i2pr emits each code are recorded as an explicit conservative policy.
i2pr owns no detector for ClockSkew, SymmetricNAT, FullConeNAT, Proxy, Mesh, or
Stan, so it never emits those codes; the honest ordinary-production baseline is
`status=2, error=0, testing=0` — no claim made, and no test running.

**Plan 339 closed `passed` on 2026-10-05**, taking Group B. The five selectors
now have real bounded owners across three layers, gating is per key (the two
`error` rows need an attested NetDB; `status.v6` and `testing.v6` do not), and
teeth were verified by inverting the status mapping and removing the family
match: 4 of 13 transport rows, both runtime rows, and 2 of the 4 wire rows
failed, and the source was restored with an empty diff. Normative vocabulary and
emission policy live in
[`specs/references/proposal-170-network-status-error-testing.md`](../../specs/references/proposal-170-network-status-error-testing.md).
Group A is the remaining Plan 322 work.

**Plan 340 registered `active` on 2026-10-05, taking Group A's ownership half.**
The split above was correct that Group A is *not* a missing snapshot, and Plan
340 takes the part that follows from that: a router that relays nothing still
has to be able to say so truthfully. `TransitParticipation` is installed by the
composition root in its `Disabled` state, which holds no counters at all — so
the production answer of `0` / `0` / `0.0` is a property of the enforced
posture rather than a default, and a participating router's numbers come from
counters the real `TunnelData` forward path advances.

The participation posture itself stays exactly where Plan 268/269 left it:
disabled, non-advertised, unclaimed. The 2026-10-05 correction in
[`265-status.md`](../../closure/transit-tunnels/265-status.md) is what makes
that non-negotiable rather than merely cautious — the retained M11 qualification
is demonstrably bound to `6ab9dc2d`, not to the current tree, so it cannot
justify a posture change here. `shareratio` additionally gets an explicit
definition, because Proposal 170 leaves its arithmetic unspecified and no
pinned reference implements it.

**Plan 334 reclosed `passed` on 2026-10-05.** Its own black-box, rollback, and restart
evidence landed in `db63bc0`, and writing it found one more real gap: Plan 337 had added the
posture and address to the *control-state* response, but the JSON-RPC adapter builds its own
`info` object and dropped them — so a real client could never discover the address, and the
whole mode mapping was unobservable outside the process. The wire step fixes that.

**Plan 335 closed `blocked` on 2026-10-05, and this line does not fully close.** The Emissary
black-box differential passed byte-exact across 90 rows. **Corrected twice on 2026-10-05.** First:
this record previously said the two references had no overlapping feature set, because a search for
the specification's `I2P_Red25519H` hash-domain literal found nothing while i2pd names the scheme
`RedDSA`. Both references implement the full ELS2/Red25519 domain. Second: **the Java lane is no
longer unprovisioned — it was executed**, and it fails the same way i2pd does. Both lanes now run
against unmodified pinned reference code at the cryptographic boundary: i2pd through its own
`IdentityEx::CreateVerifier(11)` linked against the real `libi2pd.a`, Java through the pinned
`net.i2p.crypto.eddsa` subtree compiled unmodified with `javac`, on one key and one message shared
with the committed i2pd fixture. The measured matrix: i2pd and Java **verify each other** and both
**reject i2pr**; i2pr rejects both; blinded public keys identical across all three.

The cause is a construction mismatch behind a name collision rather than a missing feature in either
reference: both implemented *Zcash RedDSA* — `RedDSAEngine`'s own class comment cites the Zcash
specification — which is I2P's Red25519 minus the `I2P_Red25519H(x)` domain and 2-byte length
framing. Because every encrypted LeaseSet2 record signs its outer layer under the blinded key,
whose sigtype is always 11, no specification-conformant type-5 record is verifiable by either named
reference. So the encrypted-LeaseSet branch is implemented and internally qualified, agrees
byte-exactly with Emissary, and is blocked on a **measured** interoperability failure that no
provisioning can clear. The second correction also retracts a first-correction error: **Emissary is
Rust, not Java** (`eepnet/emissary`, 229 `.rs` files and zero `.java`), so it is not evidence about
the Java ecosystem — the specification form has one supporting implementation, not a Java-ecosystem
consensus. Upstream reporting has not been started and is out of scope; the type-11 ELS2 path is not
exercised by the live network, so no deployed router is malfunctioning.

Plan 337 was the real blocker behind Plan 334, and it was older and broader than the ELS2 work. A
service tunnel created through TunnelManager was reconciled onto a `ServiceTunnelManager` built by
`TunnelControlState::for_config` over an *empty* `ServiceTunnelSet`, while `ServiceProduct::new`
built the one `publish_service_ls2_for_service` is handed and gave it the executable router delivery
backend. No production call site installed that capability on the control-owned manager, so a
control-created server published no LeaseSet2 at all — encrypted or ordinary. This contradicted Plan
289's own plan-of-record, which states the invariant as a requirement (lines 13 and 128) and which
Plan 289's lifecycle evidence could not observe, because those rows assert the control transaction
coordinator and never that a control-created runtime is reachable from outside the control state.
Plan 334 pinned it with two negative rows; Plan 337 made Plan 289's stated invariant hold in the
source (ADR 0031) and **deleted** those two rows rather than rewording them, replacing them with
eleven positive ones.

**Plan 338 passed, and corrected the record twice.** The composition root now builds the one
`ServiceTunnelManager` and injects the same `Arc` into both owners, `i2pcontrol` declares
`depends_on("ssu2-router")` so the product prepares the manager and installs the delivery backend
before any control reconcile, the product gate widened to cover a router whose only route to a
service tunnel is the control plane, and a type-5 record is filed at the record's own blinded storage
key with a `.b32.i2p` exposed. Two defects became reachable and are Plan 338's:

- **The ELS2 loader read a store the runtime never writes to.** Plan 337 diagnosed this as a missing
  capability — "a control-created server cannot hold a persisted identity" — and that diagnosis was
  **wrong**. `ServiceTunnelSet::destination_groups` sets `group.persistent` for `kind.is_server()`, so
  every server group is persistent and the manager always wrote a `ServiceDestinationRecord`; the
  loader read `for_service` while the runtime wrote `for_group` (a `KeyReference` policy writes a third
  path). Three store paths for one concept, and the loader had duplicated the resolution instead of
  asking the owner. The refusal itself was correct and fail-closed; only the reading of it as a missing
  capability was wrong. `ServiceTunnelManager` is now the single owner of that resolution, and both the
  Plan 337 closure and the Plan 338 plan of record carry a dated correction rather than a silent edit.
- **`rollback_state` never reconciled the manager.** A failed transaction could leave a runtime
  installed with no durable definition behind it. Pre-existing — `sync_names` and store-publish
  failures reach it too — and Plan 337 only made it the visible outcome of every refused encrypted
  create. All five call sites now await a rollback that reconciles.

Plan 335 carries three named obligations that earlier plans did not resolve: the Java I2P
authorization lane and the i2pd authorization lane, both unexecuted; the type-11 signature
transcript divergence recorded in Plan 336; and the client-count ceiling divergence recorded in
worksheet §14.23, where i2pr publishes up to 255 authorized clients and Emissary parses at most 99.

Plan 335 resolves only the encrypted-LeaseSet branch. A future full-Proposal gate still requires
successor work for blocked Plan 322 production transit/IPv6 sources and blocked Plan 327
I2P-routed outproxy/secret ownership.
