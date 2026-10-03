# Anonymity and Implementation-Neutrality Roadmap

Status: Plan 296 and Plan 307 are closed. Plans 297–305 remain historical stopped records. ADR 0030 corrects the future model: Destination groups are explicit linkability domains, router-to-Destination unlinkability is the primary invariant, HTTP evidence is separated from hostile Streaming evidence, and Streaming convergence targets pinned i2pd. Plans 308 and 309 are dependency-ready; Plans 310–313 remain behind the group-pool, lifecycle, and Streaming evidence gates. This workstream remains parallel to M12/router-mainline development.

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

## 3. Non-goals

No production-anonymity claim, Tor Browser equivalence, global passive-adversary resistance, arbitrary application fingerprint normalization, TLS MITM, or unlimited resource allocation. Intentional services within one Destination group are explicitly linkable.

## 4. Current state

Plan 296 removed known direct client-boundary leaks. Plan 304 retained an Ubuntu preflight/reference cache but over-coupled HTTP and hostile Streaming. Plan 305 correctly discovered that the production service path still has one service-owned Destination and one-peer build requests, but its mandatory per-target identity rule is superseded by ADR 0030.

Current code still needs:
- systematic server/client sanitation coverage;
- general Destination-group ownership;
- explicit server port multiplexing;
- real multi-hop service paths rather than config-only three-hop intent;
- lifecycle separation between router process and service availability;
- a practical Streaming fingerprint target.

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
  -> 308 HTTP ordinary-proxy differential [ready]
  -> 309 Destination groups + service multiplexing [ready]
       -> 310 group-owned multi-hop pools + selector
            -> 311 startup/graceful lifecycle
            -> 312 i2pd Streaming directional baseline
                 -> 313 i2pd Streaming convergence

passing 308 + 310 + 311 + 313
  -> future integrated anonymity successor to stopped Plan 301
~~~

Plans 308 and 309 may proceed independently after 307. Plans 311 and 312 may proceed independently after 310.

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
| 308 | in progress | evidence/capability | plans/implementation/anonymity/308-http-proxy-differential-without-hostile-streaming-control.md | plans/closure/anonymity/307-status.md unblocks; retained Plan 304 artifacts and ADR 0030 satisfy other dependencies |
| 309 | in progress | architecture/capability | plans/implementation/anonymity/309-destination-linkability-domains-and-service-group-composition.md | plans/closure/anonymity/307-status.md unblocks; retained Plan 305 audit and ADR 0030 satisfy other dependencies |
| 310 | blocked on 309 | architecture/anonymity capability | plans/implementation/anonymity/310-destination-group-multihop-pool-and-peer-selection.md | future |
| 311 | blocked on 310 | lifecycle/anonymity capability | plans/implementation/anonymity/311-service-lifecycle-startup-and-graceful-drain.md | future |
| 312 | blocked on 310 | evidence infrastructure | plans/implementation/anonymity/312-i2pd-streaming-directional-fingerprint-baseline.md | future |
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

Plan 307 uses transcript/static/parser negative tests. Plan 308 uses ordinary HTTP captures on Ubuntu. Plans 309/310 use deterministic composition/path tests and later mixed-router evidence where necessary. Plan 311 uses manual-time lifecycle tests. Plan 312 uses directional i2pd black-box traces with an i2pr-controlled opposite endpoint. Plan 313 reruns the same evidence after narrow tuning.

External lanes remain exact-pin/source-locked, fail-closed, bounded, and sanitized. Missing reference execution is unexecuted, never a pass.

## 10. Risks and decision points

The largest risk is accidental router linkage through service composition rather than literal headers. A second risk is making deliberate Destination sharing appear unsafe and driving unnecessary identity churn. A third is implementing "three-hop" only in configuration while production still constructs one-hop paths. Lifecycle smoothing must not keep a compromised/failed transport alive merely to preserve availability.

## 11. Completion definition

The workstream is complete only when direct service-boundary leaks are absent; HTTP's qualified profile has retained evidence; Destination groups intentionally compose multiple services without router identity reuse; production group pools build real qualified paths; lifecycle availability is decoupled from immediate process edges; i2pr Streaming matches pinned i2pd on registered observable dimensions; and a fresh integrated evidence plan states only those scoped properties.

## 12. Milestone status summary

Plan 307 is the only dependency-ready anonymity handoff at registration. Plans 308–313 are registered in the graph above. Plans 297–305 remain immutable stopped history. No mainline/M12 readiness or production-anonymity claim changes.
