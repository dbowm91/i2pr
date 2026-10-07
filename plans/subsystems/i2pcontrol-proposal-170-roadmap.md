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

Plans 321, 323, and 325 were independently eligible after Plan 320. Plan 325's packaged-provider survey closed blocked, then the clean-room successor line 329–331 implemented and qualified the primitive. Plans 332–334 implemented type-5 ELS2, client authorization, and the canonical control mapping; Plans 337/338 corrected product composition. Historical Plan 322's source gaps are now closed by passed successors 339/340. Historical Plan 327's secret/provider gaps are now partly superseded by 341/343, with request-path/option/wire capability still owned by ready Plan 342. Plan 335 measured a genuine type-11 incompatibility, but subsequent source-history research reclassifies it as a specification/deployment split: Proposal 146/standalone Red25519 uses domain+length-framed HStar, while the Encrypted-LS2 specification and deployed Java/i2pd use randomized RedDSA without those additions. Plans 346→347 own the bounded ELS2 correction and live cross-router proof. Historical Plan 328 remains blocked; fresh final gate Plan 348 waits on 342+347. The unqualified full Proposal 170 claim is still not made.

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

344 passed   Plan 326 re-audit
  (The re-audit 326's own record recommended, run against its actual
   acceptance criteria. TWO findings. First, 326's recorded blocker
   was SUPERSEDED: the token named Plan 325's Red25519 provider, which
   Plan 331 already answered (passed); the token is corrected. Second,
   a REAL gap: of the ten canonical EncryptLeaseSet spellings,
   "encrypted with per-user key (psk)" had only ever passed at the
   PARSER -- no ELS2 material row, no type-5 record row -- and 326 says
   "no mode may pass from parser acceptance or inert storage". The
   frozen mapping's claim that the spelling is covered by its
   behavioural twin is the exact reasoning that forbids; the mapping is
   the design, not the evidence. Fixed in both publication-path rows
   with three client authorizations, and an inversion breaking its
   mapping fails exactly those two rows and no others. Test-only.
   CONSEQUENCE: 326's remaining gap is EXTERNAL, so Plan 328 cannot be
   unblocked by any purely local plan. See the 326 rows below.)

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

342 passed (scoped)  outproxy option surface + request paths + wire lane
  (Plan 342 retains the remainder after Plan 343: ProxyList /
   UseOutproxyPlugin / OutproxyAuth / OutproxyUsername /
   OutproxyPassword / OutproxyType / SSLProxies semantics with
   transactional reconfiguration, HTTP / CONNECT request-path
   integration and the Proposal-applicable SOCKS families, and the
   self-composed in-tree loopback outproxy wire lane -- loopback
   evidence, NOT interoperability, per the Plan 339 decision.

   STEPS 1-3 LANDED. Step 1 closed the two structural gaps Plan 342 named:
   `RouterIdentityBundle` gained a closure-based signing-seed accessor, and
   the outbound secret store is derived once at the composition root and
   threaded as a single `Arc`, so no second key exists. `OutproxyRoute` gained
   a `Refused(OutproxyFailure)` variant, because the enum previously had no
   way to say "no" and an empty proxy list made it claim `DirectI2p` for a
   clearnet target. Step 2 admitted all seven canonical fields as ONE block:
   a partial block is refused by name before any allocation, `outproxy_type`
   is a closed vocabulary, `OutproxyPassword` is sealed into the Plan 341
   stored form by `normalize_definition` and refuses the tunnel outright when
   no owner is installed. Step 3 made the provider reachable: one
   `classify_client_target` decides Direct / ViaOutproxy / Refused, all three
   request paths match it exhaustively before opening anything, and the
   outproxy handshake prefix is carried into the pump's INBOUND direction --
   `run_stream_pump`'s `initial_bytes` feeds the opposite one, so putting it
   there would have corrupted the first request on every pipelining outproxy.

   STEP 4 LANDED, AND IT FOUND THAT STEPS 2-3 SHIPPED UNREACHABLE. Every
   request-target grammar in the tree (Plan 176 HTTP absolute + authority form,
   Plan 290 SOCKS5 + SOCKS4a) hard-required a `.i2p` suffix and refused a
   clearnet authority BEFORE `classify_client_target` ran: `CONNECT
   example.com:443` was answered 403 by the PARSER, and a SOCKS5 clearnet
   target was answered `HostUnreachable` from inside the NEGOTIATOR. So
   `ClientTargetClass::ViaOutproxy` was dead in production while its unit rows
   -- which call the classifier directly -- passed. That is this plan's own
   inert-acceptance failure one level deeper than the plan anticipated: not an
   option surface with no route behind it, but a route with no reachable input.
   A shape can be right while the path is dead, and only running it says so.

   Fixed by `TargetPolicy { I2pOnly, AllowsClearnet }`, where `I2pOnly` is the
   default and every pre-342 entry point is a thin wrapper over it, so nothing
   widened by accident and a caller must NAME the relaxed policy. Only the
   suffix requirement became policy-dependent: IP literals stay refused (a
   numeric authority has no name to apply a Host policy to, so allowing it
   would make the tunnel an open relay by address), `localhost` stays refused
   (a foreign resolver must not be asked for loopback), and userinfo, control
   bytes, ceilings, zero port and scheme/form grammar are untouched.
   Mixed-suffix confusion is the classifier's decision, not the parser's.
   `ServiceTunnelManager::target_policy(spec_id)` is the single decision point
   and reads the PROVIDER REGISTRY, not the options value, so a provider
   removed between reconciliation and a request cannot leave the parser and
   the classifier disagreeing.

   SCOPING SETTLED: `handle_proxy_request` IS in step 3's scope. The
   seven-field block is admitted on `httpclient`, and a forward path that
   ignored it would be inert acceptance at sub-path granularity -- accepted on
   the kind, honoured on one of its two request forms. It is not a copy of the
   direct path either: after `build_attempt` a session is a byte pipe to the
   ORIGIN, not a forward proxy, so the request must be origin-form carrying
   the CLEARNET authority in `Host:` and never the `b32.i2p` substitution the
   direct path makes.

   TWO MORE REAL DEFECTS, both found only by running the lane: the outproxy
   opener never called `notify_outbound_signal`, so the queued SYN was not
   routed and every route would have failed with `TargetUnreachable` in
   production; and a control commit publishes `startup` + control-owned
   definitions, so an outproxy endpoint living only in the manager's spec set
   is dropped by the first `create` -- it must be startup-owned.

   EVIDENCE: `crates/i2pr-daemon/tests/outproxy_loopback_wire.rs`, 8/8 rows.
   The outproxy is reached through STREAMING via a service server tunnel whose
   `ServerTarget` is `LoopbackTcp`, exactly as production reaches one, and the
   fixture maps the requested authority onto a loopback origin it was given at
   construction -- it never resolves a name, so the test process itself never
   holds a clearnet capability, which would invert invariant 1 at the layer
   meant to enforce it. Guards: `check-outproxy-request-path.sh` extended
   24 -> 39/39 mutations, plus new `check-outproxy-wire-lane-evidence.sh` at
   7/7 mutations with 2/2 deliberate NON-FLAGGING controls (comment-stripping
   has a failure mode in the other direction too).

   PLAN 327 STAYS BLOCKED, on a narrower thing than it was: the live failover
   rotation between two configured outproxies, and a live restart carrying a
   request, are UNPROVEN. Two rows were written for them and then REMOVED
   rather than left ungreen; their names are machine-checked in
   `DOCUMENTED_ABSENCES` so the absence cannot rot into a silent claim. No
   interoperability evidence: the Java 2.13.0 and i2pd 2.61.0 pins are
   untouched, and a loopback fixture is not an outproxy that answers.)

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


## 15. ELS2 deployment reconciliation and fresh final gate (Plans 346–348)

The historical Plan-335 measurements are retained, but their earlier interpretation is superseded
for forward execution.

Research frozen before registration established:

- Java I2P's RedDSA engine landed on 2019-02-20.
- Proposal 146's first committed text on 2019-02-24 already contained
  `I2P_Red25519H(x)` and two-byte message-length framing.
- the Encrypted LeaseSet specification documents the randomized RedDSA form without those
  Proposal-146 additions;
- i2pd implemented that deployed form on 2019-03-24;
- current Java and i2pd verify each other's type-11 signatures.

Therefore the open problem is not "ELS2 does not work cross-router" and not simply "both references
are wrong". It is that i2pr used the standalone Proposal-146 transcript for a network object whose
deployed Java/i2pd ecosystem follows the Encrypted-LS2-spec transcript.

Forward graph:

```text
346 passed
  ELS2-only type-11 transcript authority / deployed-compatibility corrective
    -> 347 stopped (classified boundary; 0 of 4 directions)
       real bidirectional Java+i2pd type-5 publication/lookup/application qualification
       (its i2pr-side correctives 350 and 349/351 have since passed, so the
        remaining work is reference-side harness plus the live lane)

342 passed (scoped)
  outproxy option surface + HTTP/CONNECT/SOCKS request paths + wire evidence
    (option surface and request paths landed together, because the plan
     forbids accepting the surface before the route behind it exists; the
     loopback wire lane then landed and found that BOTH were unreachable,
     because every request-target grammar refused a clearnet authority
     before the classifier ran. Fixed by TargetPolicy. The live failover
     rotation and a live restart remain unproven, so Plan 327 stays blocked
     and no outproxy capability is claimed)

342 passed (scoped) + 347 passed
  -> 348 fresh Proposal-170 full-conformance gate
       (re-freeze the then-current Open Proposal before any final claim)
```

Plan 346 must leave the standalone Proposal-146 Red25519 primitive and all official vectors
unchanged. Any deployed compatibility verifier/signing mode is confined to typed ELS2 type-5
contexts; generic type-11 dual verification is forbidden.

Plan 347, not a cryptographic fixture, owns the evidence needed to close historical Plan 326:
actual DatabaseStore type-5 publication, independent NetDB lookup, outer verification/decryption,
inner LeaseSet2 validation, and a successful streaming/application exchange in both directions with
stock Java I2P and i2pd.

Plan 348 replaces historical blocked Plan 328 for forward execution. Plan 328 is not rewritten.

| Plan | State | Classification | Handoff |
|---|---|---|---|
| 346 | passed | protocol/security corrective (ADR 0032, Proposal 170) | plans/implementation/i2pcontrol-proposal-170/346-els2-type11-transcript-deployed-compatibility-corrective.md |
| 347 | stopped at a classified boundary, 0 of 4 directions; its i2pr-side correctives (350, 349/351) have since passed, so its remaining work is reference-side harness plus a live lane | external interoperability/capability closure | plans/implementation/i2pcontrol-proposal-170/347-live-bidirectional-els2-cross-router-qualification.md |
| 348 | **blocked historical**; its dependency model is now stale — 347 stopped rather than being able to pass, and 342 is passed-*scoped* rather than passed. Superseded forward by **Plan 378** | final conformance/evidence gate | plans/implementation/i2pcontrol-proposal-170/348-fresh-full-proposal170-conformance-gate.md |

## 16. Consumer-path correctives and config hygiene (Plans 349–352)

Plan 347 stopped at a classified boundary with 0 of 4 directions, and named two independent causes:
i2pr had **no type-5 consumer path** at all, and stock Java I2P would not dial i2pr. Those are
separable, so the i2pr-side half was corrective rather than left to a lane that could never run.

**Both causes are corrected forward, and Plan 373 records that correction.** (a) The consumer-path
gap is closed: Plan 351 gave both ELS2 resolvers production callers reached through a
service-tunnel remote target. (b) The Java blocker was **not** a bandwidth-tier design, which is
how 347 recorded it. It is a *tunnel-peering* gate — `TunnelPeerSelector.shouldExclude` caps arity
plus `allowAsIBGW`'s `R` requirement — and neither Java row requires Java to select i2pr as
OBEP/IBGW/participant. The queried-floodfill topology of Plans 303/306 already reaches i2pr as a
floodfill. ADR 0030 is untouched and no tier letter is needed or invented.

| Plan | State | Classification | Handoff |
|---|---|---|---|
| 349 | superseded by 351 (its bounded owner landed; only the production caller was missing) | capability corrective | plans/closure/i2pcontrol-proposal-170/349-status.md (`superseded-by-plan351-with-caller-landed`) |
| 350 | passed | capability/corrective | plans/closure/i2pcontrol-proposal-170/350-status.md |
| 351 | passed (gate-scoped) | capability corrective — supplies the missing production caller | plans/closure/i2pcontrol-proposal-170/351-status.md (`passed-gate-scoped-els2-consumer-service-wiring-landed`) |
| 352 | passed | invariant + security corrective — config secret hygiene | plans/closure/i2pcontrol-proposal-170/352-status.md (`passed-structural-toml-error-redaction-and-conditional-at-rest-mode-gate`) |

Plan 351 made a `.b33` address a service-tunnel remote target under three measured gates
(`DelayOpen`-only, I2PControl-only, no inline config secret), which closed the i2pr-side consumer
half that Plan 347 identified.

Plan 352 is **independent of that capability work** and of Plan 349 — it was found while scoping
351's Gate 3 and was filed separately so 351's scope stayed honest about not owning the defect. It
is a pre-existing leak, live today, unrelated to any ELS2 change. Both paths are closed
**structurally**: a probe proved `deny_unknown_fields` leaks the offending key through `toml`'s own
`message()`, so message filtering would have shipped a guard that passes while the secret still
reaches stderr. `RedactedTomlError` resolves the byte span to a line/column pair, retains no
content, and keeps the upstream error reachable via `Error::source`. The matching at-rest gap is
closed by a conditional `& 0o077` gate in `Config::load`, whose decision is factored into a
platform-independent function precisely so the **non-POSIX refusal is testable on a POSIX host**.
That refusal is a deliberate Windows behaviour change: a password-bearing config now fails closed
rather than loading on a platform that cannot be examined. No CI evidence exists for AC8; the local
routine floor is 46/46 and the residual `Debug`/`Clone` exposures plus one observed load-sensitive
flake are recorded in the closure record rather than smoothed over.

**Where the Proposal 170 line now stands.** Plan 352 was the last plan in this line whose work does
not require the external Java/i2pd lane. Everything still open here is blocked on external
evidence, not on local work: Plan 347 is stopped at the classified boundary, Plan 348 is blocked on
347 alone (its §1 re-freeze is executed and clean), and Plans 325/326/327/328 remain blocked.
**Plan 373 reconciled this paragraph forward**, because the sentence above it had drifted: it still
described the pre-350/351 world in which i2pr had no type-5 consumer path and Java needed a
bandwidth tier, and its “Plan 348 is blocked on 347 alone” claim reads as though 347 were a plan
that could pass. It is stopped. Plans 325/326/327/328 stay blocked as immutable historical records
with successors named in §17.

**No Encrypted LeaseSet2 interoperability or full-Proposal claim is promoted. Type 5 stays
non-advertised.** Plan 352 hardened the configuration path; it changed no protocol behaviour, no
capability advertisement, and no support surface.


## 17. External qualification and terminal closure phase (Plans 373–378)

The previous final-gate graph is superseded for forward execution by this section. Historical
closures remain unchanged.

Current facts:
- Proposal 170/346 closed the ELS2 type-11 transcript policy.
- Proposal 170/350 and /351 closed the missing i2pr type-5 floodfill/consumer plumbing.
- Proposal 170/347 therefore no longer describes a current local blocker; it remains the stopped
  historical attempt with 0/4 external directions.
- Proposal 170/342 passed scoped but explicitly left live multi-outproxy failover and a post-restart
  routed request unproven.
- Proposal 170/348 is a historical blocked final gate. Its Proposal re-freeze was clean.

Forward dependency graph:

```text
373 passed  planning/support truth reconciliation
  -> 374 blocked  stock i2pd bidirectional ELS2 lane (freeze done; driver not written)
  -> 375 blocked  stock Java I2P bidirectional ELS2 lane (freeze partial; driver not written)
  -> 376 passed  live outproxy failover + restart
       |
       +-> 377 blocked  ELS2 external convergence (needs 374 + 375)
            |
            +-> 378 blocked  final conformance gate (needs 377; 373/376 done)

374 + 375
  -> 377 blocked  ELS2 external evidence convergence / successor closure

373 + 376 + 377
  -> 378 blocked  final Proposal-170 conformance gate
```

Plans 374, 375, and 376 are intentionally independent after 373 and may execute in parallel.
**Plan 373 passed**, so all three became `ready`: it was their only unmet hard dependency — each
of 374/375 also requires passed 346/350/351, and 376 requires passed 341/343 plus 342-passed-scoped.

**376 has since passed, and 374/375 are blocked.** Both ELS2 lanes completed their
pre-execution freeze and then stopped at the same named point: the repository has no
ELS2 live driver. It has the Plan 303/306 controlled floodfill mesh (type 1/3/7 only),
Plan 350/351's i2pr-internal type-5 path, and Plan 346's crypto-boundary transcript check —
none of which is a live ELS2 row, and the plans forbid substituting any of them. The mesh
itself was **verified running on this host**, so the external lane is not blocked by the
environment; it is blocked by unwritten external-integration engineering. The freeze record
lives in `tests/integration/els2/reference-freeze.md` so the next pass does not repeat it.

### Why two ELS2 reference plans

Java and i2pd share the deployed type-11 transcript but have different configuration/harness
surfaces. One combined plan would make it possible for one reference family to consume the entire
execution budget before the other direction was constructed. Plans 374/375 each own exactly two of
the four mandatory directions. Plan 377 is the only place allowed to converge them into an external
ELS2 claim.

### Why outproxy has a successor after scoped Plan 342

Proposal 170/342 deliberately removed rather than waived two ungreen rows: multi-endpoint live
failover and post-restart request success. Plan 376 owns exactly those residuals. A scoped pass is
not a Plan-327 closure.

### Final claim

Historical Plans 328 and 348 remain blocked records. Plan 378 is the only forward terminal gate and
the only plan in this phase allowed to set `full-proposal-conformant`.

| Plan | State | Classification | Handoff |
|---|---|---|---|
| 373 | **passed** | planning/support corrective | plans/closure/i2pcontrol-proposal-170/373-status.md (`passed-authority-support-and-successor-state-reconciled-without-capability-promotion`) |
| 373 (plan) | — | — | plans/implementation/i2pcontrol-proposal-170/373-prop170-authority-support-reconciliation.md | plans/implementation/i2pcontrol-proposal-170/373-prop170-authority-support-reconciliation.md |
| 374 | **blocked** | external interoperability | plans/closure/i2pcontrol-proposal-170/374-status.md. Freeze executed; controlled mesh verified running here; both directions reference-feasible and all three auth modes implemented. Blocker: no ELS2 live driver exists. | plans/implementation/i2pcontrol-proposal-170/374-i2pd-live-els2-qualification.md |
| 375 | **blocked** | external interoperability | plans/closure/i2pcontrol-proposal-170/375-status.md. Build verified at the pin (JDK 21 required); source proof recorded incomplete; same unwritten-driver blocker. | plans/implementation/i2pcontrol-proposal-170/375-java-live-els2-qualification.md |
| 376 | **passed** | capability/resilience closure | plans/closure/i2pcontrol-proposal-170/376-status.md (`passed-live-multi-endpoint-failover-and-product-restart-proven-plan327-remainder-closed`) | plans/implementation/i2pcontrol-proposal-170/376-outproxy-live-failover-restart-closure.md |
| 377 | blocked on 374 + 375 | ELS2 external convergence. Closure: `plans/closure/i2pcontrol-proposal-170/377-status.md`. Did not pass and cannot: both inputs absent, so all four directions have no executed row. | plans/implementation/i2pcontrol-proposal-170/377-els2-external-evidence-convergence.md |
| 378 | blocked on 377 alone (373 + 376 passed) | final conformance gate. Closure: `plans/closure/i2pcontrol-proposal-170/378-status.md`. §1 re-freeze executed live and MET; §2/§5 green; §3/§4 blocked; `full-proposal-conformant` NOT SET. | plans/implementation/i2pcontrol-proposal-170/378-final-prop170-conformance-gate.md |
