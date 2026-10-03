# Anonymity and Implementation-Neutrality Roadmap

Status: Plans 296, 307, and 309 are closed. Plan 308 is independently blocked on the controlled ordinary-HTTP peer topology and three-family captures. Plan 310 remains the authoritative blocked record for the one-peer service-build/group-pool defect. Corrective Plan 314 is dependency-ready and owns the production multi-hop request, validated-NetDB selector, and deterministic three-hop cryptographic proof; Plan 315 is blocked on 314 and owns Destination-group pool integration. Plans 311 and 312 are now blocked on Plan 315 rather than requiring a rewritten Plan 310 record; Plan 313 remains blocked on Plan 312. Plans 297–305 remain historical stopped records. ADR 0030 establishes explicit Destination linkability domains, router-to-Destination unlinkability, separates HTTP evidence from hostile Streaming evidence, and targets pinned i2pd for Streaming convergence. This workstream remains parallel to M12/router-mainline development.

Long-term references:
- GUARDRAILS.md
- specs/CONFORMANCE.md
- specs/support.toml
- docs/security-model.md
- docs/adr/0029-anonymity-boundaries-and-profile-convergence.md
- docs/adr/0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md
- plans/subsystems/service-tunnels-roadmap.md
- plans/subsystems/destination-streaming-roadmap.md
- plans/subsystems/netdb-roadmap.md

Reference pins:
- i2pd 2.61.0 commit 635b013a612ff47278ef02acf8580a28e10e26c5.
- Java I2P 2.13.0 commit 9134f808337b401e8e53c73734c81fab04280c9d.
- Plan 215 remains M10 service product authority.
- Plan 193 remains retained i2pd Streaming interoperability authority for its historical scope.

## 1. Purpose and ownership boundary

Prevent service traffic from identifying or correlating its hosting router while allowing intentional application-level linkage through explicit Destination groups.

A Destination is allowed to identify several services because the user chose that group. The router identity is not part of that group. Application/service crates own parsing/presentation policy; i2pr-client owns Destination/Streaming state; i2pr-netdb supplies validated peer facts; i2pr-daemon owns group/runtime composition; i2pr-tunnel owns final wire/path validation.

## 2. Work classification

- Plan 296: passed direct branding/local-alias corrective.
- Plans 297–305: retained stopped evidence from the superseded planning model.
- Plan 307: immediate router-unlinkability and input-sanitation invariant.
- Plan 308: HTTP ordinary-proxy differential evidence/convergence.
- Plan 309: Destination-group composition and service multiplexing.
- Plan 310: real group-owned multi-hop pools and peer selection.
- Plan 311: startup separation and graceful retirement lifecycle.
- Plan 312: i2pd-only directional Streaming baseline.
- Plan 313: evidence-driven i2pd Streaming convergence.
- Plan 314: corrective multi-hop production build contract, validated-NetDB selector, and deterministic three-hop proof.
- Plan 315: corrective Destination-group pool ownership and Destination-operation integration.

## 3. Non-goals

No production-anonymity claim, Tor Browser equivalence, global passive-adversary resistance, arbitrary application fingerprint normalization, TLS MITM, or unlimited resource allocation. Intentional services within one Destination group are explicitly linkable.

## 4. Current state

Plan 296 removed known direct client-boundary leaks. Plan 304 retained an Ubuntu preflight/reference cache but over-coupled HTTP and hostile Streaming. Plan 305 correctly discovered that the production service path still has one service-owned Destination and one-peer build requests, but its mandatory per-target identity rule is superseded by ADR 0030.

Current code still needs:
- a production Destination build request that carries a complete selected path rather than one peer;
- group-owned Destination pools consuming that established multi-hop material for LeaseSet/data/lookup/publication;
- lifecycle separation between router process and service availability;
- a practical Streaming fingerprint target.

Plan 310's failed pass established that these first two items are architecture boundaries, not a requirement to stand up an external three-router i2pd network. The corrective sequence therefore uses deterministic validated RouterInfo fixtures and the real short-build cryptographic state machine for Plans 314–315; live multi-router topology may be added later as non-gating interoperability infrastructure.

## 5. Target architecture

~~~text
router identity / router transport plane
        |
        | MUST NOT become service identity
        v
DestinationGroup owner
  identity + LeaseSet + inbound/outbound pools
  lifecycle + publication + delivery + budgets
        |
        +-- HTTP client #1
        +-- HTTP client #2
        +-- SOCKS client
        +-- IRC client
        +-- server port 80 -> backend A
        +-- server port 443 -> backend B
        |
        v
3-hop group-owned client tunnels
        |
        v
I2P network

Services in one group are intentionally linkable to each other.
The group is not linkable by design to the hosting RouterInfo.
~~~

## 6. Dependency graph

~~~text
296 passed

307 service-boundary router unlinkability + sanitation [passed]
  -> 308 HTTP ordinary-proxy differential [blocked: controlled topology]
  -> 309 Destination groups + service multiplexing [passed]
       -> 310 original multi-hop/pool plan [blocked historical record]
            -> 314 multi-hop request + selector + deterministic 3-hop proof [ready corrective]
                 -> 315 group-owned pool + Destination-operation integration [blocked on 314]
                      -> 311 startup/graceful lifecycle
                      -> 312 i2pd Streaming directional baseline
                           -> 313 i2pd Streaming convergence

passing 308 + 315 + 311 + 313
  -> future integrated anonymity successor to stopped Plan 301
~~~

Plan 308 proceeded independently and remains blocked on HTTP topology evidence; Plan 309 passed independently of it. Plan 310 remains an immutable blocked record. Plan 314 is the dependency-ready corrective for its build-contract/selector half; Plan 315 follows for the group-pool/consumer half. Plans 311 and 312 may proceed independently after Plan 315 passes.

## 7. Milestones

| Plan | State | Classification | Handoff | Closure |
|---|---|---|---|---|
| 296 | closed | invariant/corrective | plans/implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md | plans/closure/anonymity/296-status.md |
| 297 | stopped | historical HTTP evidence | plans/implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md | plans/closure/anonymity/297-status.md |
| 298 | stopped | historical Streaming evidence | plans/implementation/anonymity/298-streaming-active-fingerprint-differential-harness.md | plans/closure/anonymity/298-status.md |
| 299 | stopped | historical Streaming convergence | plans/implementation/anonymity/299-streaming-observable-profile-convergence.md | plans/closure/anonymity/299-status.md |
| 300 | stopped | historical target/path qualification | plans/implementation/anonymity/300-destination-isolation-and-tunnel-path-anonymity-qualification.md | plans/closure/anonymity/300-status.md |
| 301 | stopped | historical integration gate | plans/implementation/anonymity/301-integrated-anonymity-evidence-and-scoped-claim-gate.md | plans/closure/anonymity/301-status.md |
| 304 | stopped | retained Ubuntu/reference foundation | plans/implementation/anonymity/304-ubuntu-controlled-reference-topology-and-capture-foundation.md | plans/closure/anonymity/304-status.md |
| 305 | stopped | retained owner/reference audit | plans/implementation/anonymity/305-target-scoped-destination-and-peer-diversity-ownership.md | plans/closure/anonymity/305-status.md |
| 307 | passed-service-boundary-router-unlinkability-and-input-sanitation | invariant/corrective | plans/implementation/anonymity/307-service-boundary-router-unlinkability-and-input-sanitation.md | plans/closure/anonymity/307-status.md |
| 308 | blocked on controlled HTTP reference topology | evidence/capability | plans/implementation/anonymity/308-http-proxy-differential-without-hostile-streaming-control.md | plans/closure/anonymity/308-status.md; capture tooling exists, but controlled ordinary Destination and three-family captures remain absent |
| 309 | passed-destination-linkability-domain-service-group-composition | architecture/capability | plans/implementation/anonymity/309-destination-linkability-domains-and-service-group-composition.md | plans/closure/anonymity/309-status.md; explicit group owner, persistence migration, and server-port composition passed |
| 310 | blocked-service-product-has-no-bounded-multipath-candidate-owner | architecture/anonymity capability | plans/implementation/anonymity/310-destination-group-multihop-pool-and-peer-selection.md | `plans/closure/anonymity/310-status.md`; immutable blocked record: one-peer request/provisioning path remains, with no group-owned multipath lifecycle |
| 314 | registered-plan310-multihop-build-corrective-ready | architecture/anonymity capability foundation | plans/implementation/anonymity/314-plan310-multihop-build-contract-and-deterministic-proof.md | ready; owns multi-hop request, validated-NetDB selector, exact selector-to-submission continuity, and deterministic real-crypto three-hop proof; no external topology gate |
| 315 | blocked on 314 | architecture/anonymity capability | plans/implementation/anonymity/315-plan310-destination-group-pool-integration.md | owns canonical group pool, replenish/expiry, LeaseSet derivation, and lookup/publication/data consumers; passing satisfies remaining Plan 310 requirements |
| 311 | blocked on 315 | lifecycle/anonymity capability | plans/implementation/anonymity/311-service-lifecycle-startup-and-graceful-drain.md | current dependency authority is Plan 315 corrective closure |
| 312 | blocked on 315 | evidence infrastructure | plans/implementation/anonymity/312-i2pd-streaming-directional-fingerprint-baseline.md | current dependency authority is Plan 315 corrective closure; external i2pd remains appropriate here |
| 313 | blocked on 312 | convergence capability | plans/implementation/anonymity/313-i2pd-streaming-profile-convergence.md | future |

## 8. Cross-cutting requirements

- No router-synthesized service bytes expose router identity or router-local state.
- Destination sharing is explicit; implicit sharing under resource pressure is forbidden.
- Multiple same-kind services are valid within hard ceilings.
- Server services may share one persistent group on distinct I2P ports.
- Client/server sharing in one persistent group is allowed but explicitly linkable.
- Group identity keys are independent of router keys.
- Service lookup/publication/data uses group tunnels.
- Real path length must match the configured qualified profile.
- Service lifecycle smoothing is bounded and never blocks hard security shutdown.
- Streaming qualification targets one coherent i2pd profile, not a Java/i2pd hybrid.

## 9. Verification strategy

Plan 307 uses transcript/static/parser negative tests. Plan 308 uses ordinary HTTP captures on Ubuntu. Plan 309 uses deterministic composition tests. Plan 314 proves exact three-hop selection/submission and established material with validated synthetic RouterInfos plus the production short-build cryptographic state machine; an external three-router network is deliberately not a closure requirement. Plan 315 uses deterministic/manual-time group-pool and Destination-consumer integration tests. Plan 311 uses manual-time lifecycle tests. Plan 312 uses directional i2pd black-box traces with an i2pr-controlled opposite endpoint. Plan 313 reruns the same evidence after narrow tuning.

External lanes remain exact-pin/source-locked, fail-closed, bounded, and sanitized. Missing reference execution is unexecuted, never a pass. A future isolated multi-router i2pd testnet may provide additional interoperability evidence, but it must not retroactively become a Plan 314/315 closure gate.

## 10. Risks and decision points

The largest risk is accidental router linkage through service composition rather than literal headers. A second risk is making deliberate Destination sharing appear unsafe and driving unnecessary identity churn. A third is implementing "three-hop" only in configuration while production still constructs one-hop paths. A fourth is confusing live multi-router topology availability with proof of the local selection/build contract; Plans 314–315 explicitly separate those evidence layers. Lifecycle smoothing must not keep a compromised/failed transport alive merely to preserve availability.

## 11. Completion definition

The workstream is complete only when direct service-boundary leaks are absent; HTTP's qualified profile has retained evidence; Destination groups intentionally compose multiple services without router identity reuse; Plan 314 proves production exact-three selection/build semantics; Plan 315 makes those paths the authoritative group pools and Destination-operation routes; lifecycle availability is decoupled from immediate process edges; i2pr Streaming matches pinned i2pd on registered observable dimensions; and a fresh integrated evidence plan states only those scoped properties.

## 12. Milestone status summary

Plans 307 and 309 passed. Plan 308 remains independently blocked on ordinary HTTP topology evidence. Plan 310 remains an immutable blocked record at the service-product candidate/path/pool boundary. Plan 314 is ready as the narrowed multi-hop build/selector corrective; Plan 315 is blocked on 314 and owns group-pool/consumer integration. Plans 311 and 312 are blocked on Plan 315, and Plan 313 remains blocked on Plan 312. Plans 297–305 remain immutable stopped history. No mainline/M12 readiness or production-anonymity claim changes.
