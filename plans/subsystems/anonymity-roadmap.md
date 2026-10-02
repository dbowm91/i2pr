# Anonymity and Implementation-Neutrality Roadmap

Status: Plan 296 closed; Plans 297 and 298 stopped at external capture-topology gates; Plan 299 stopped without Plan 298 differential evidence; Plan 300 stopped at target-isolation/path-diversity owner gaps; Plan 301 stopped pending predecessor qualification. This workstream is parallel to M12/router-mainline development and does not gate router feature progression.

Long-term references:
- GUARDRAILS.md
- specs/CONFORMANCE.md
- specs/support.toml
- docs/security-model.md
- docs/adr/0029-anonymity-boundaries-and-profile-convergence.md
- plans/subsystems/service-tunnels-roadmap.md
- plans/subsystems/destination-streaming-roadmap.md
- plans/subsystems/exploratory-tunnels-roadmap.md
- plans/subsystems/netdb-roadmap.md

Reference pins at registration:
- i2pd 2.61.0 commit 635b013a612ff47278ef02acf8580a28e10e26c5.
- Java I2P 2.13.0 commit 9134f808337b401e8e53c73734c81fab04280c9d.
- Existing i2pr M10 product authority: Plan 215.
- Existing mixed-router i2pd progression authority: Plan 193.

## 1. Purpose and ownership boundary

This workstream prevents i2pr-specific implementation identity from leaking or becoming unnecessarily classifiable across service/Destination anonymity boundaries, then qualifies lower-layer observable profiles that a hostile Destination or network participant can measure.

It does not own the router feature roadmap. Existing subsystem owners remain authoritative: i2pr-service-tunnels owns runtime-neutral application policies, i2pr-client owns Destination/Streaming behavior, i2pr-tunnel owns tunnel/path primitives, i2pr-netdb owns provenance/namespace policy, and i2pr-daemon owns runtime composition/listeners.

## 2. Work classification

Plan 296 is invariant + immediate corrective capability: remove direct router branding/local-alias leakage and establish the leak-regression harness.

Plan 297 is application-profile capability/evidence: converge HTTP behavior on pinned deployed I2P proxy behavior without combinatorial defaults or randomization.

Plan 298 is infrastructure/evidence only: build a hostile-Destination Streaming fingerprint harness and freeze observable Java/i2pd baselines before tuning production behavior.

Plan 299 is capability/corrective: use Plan 298 evidence to converge i2pr's observable Streaming profile on one coherent compatibility target.

Plan 300 is invariant/capability/evidence: harden Destination isolation plus tunnel/path diversity and qualify locally testable router-to-Destination unlinkability properties.

Plan 301 is evidence/closure/polish: integrate all dimensions into a machine-checked anonymity qualification matrix and update documentation only to the exact proven scope.

## 3. Non-goals

This workstream does not claim production anonymity, global passive-adversary resistance, resistance to arbitrary timing analysis, or Tor Browser equivalence. It does not MITM TLS, normalize arbitrary encrypted application protocols, spoof another router's product/version, falsify RouterInfo protocol capability/version fields, make randomization the primary fingerprint defense, or add parallel Destination/NetDB/tunnel runtimes. It does not block M12 or later mainline plans.

## 4. Current state

At registration the audit found:
- HTTP defaults replace User-Agent with `i2pr/0.1`;
- HTTP Host rewriting can use caller-local alias text instead of resolved Destination B32;
- local CONNECT success includes `Proxy-Agent: i2pr`;
- IRC's optional stable reason replacement uses `i2pr`;
- deterministic lowercase HTTP header serialization/order/connection behavior may remain distinguishable after literal branding is removed;
- browser/locale/platform headers are not comprehensively normalized;
- generic and SOCKS/CONNECT forwarding do not intentionally inject router metadata but necessarily pass application fingerprints such as TLS behavior;
- service client destinations default to dedicated identities, sharing is explicit;
- Destination identity/key shape is intentionally compatible with deployed I2P families;
- NetDB provenance/namespace separation and tunnel-routed client lookup/publication are strong unlinkability foundations;
- current Streaming contains observable defaults that differ from reference/documented behavior but lacks active black-box equivalence evidence;
- the service-Destination client tunnel profile is shorter than the common documented three-hop client-tunnel default;
- path-selection diversity is not yet a separately qualified anonymity contract.

The global security model correctly retains the anonymity/privacy non-claim.

## 5. Target architecture

~~~text
local application
     |
service-tunnel profile
     |  no router branding / local alias leakage
     v
client Destination identity owner
     |  dedicated or explicitly isolated
Streaming / Garlic
     |  qualified observable compatibility profile
client tunnel pool
     |  qualified path/diversity policy
I2P network
     |
hostile remote Destination / observer
~~~

Evidence flows back into a machine-readable qualification matrix. No individual layer promotes itself to a broad anonymity claim.

## 6. Dependency graph

~~~text
296 service-boundary invariant + direct leak correction
  -> 297 HTTP profile convergence
  -> 298 Streaming differential fingerprint harness
       -> 299 Streaming profile convergence
  -> 300 Destination/path isolation + diversity qualification

297 + 299 + 300
  -> 301 integrated anonymity evidence and scoped claim gate
~~~

Plans 297, 298, and 300 may execute concurrently once Plan 296 is closed.

## 7. Milestones

| Plan | State | Classification | Handoff | Closure |
|---|---|---|---|---|
| 296 | closed (`passed-anonymity-service-boundary-implementation-neutrality-and-leak-regression`) | invariant/corrective capability | plans/implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md | plans/closure/anonymity/296-status.md |
| 297 | stopped (`stopped-http-differential-capture-needs-controlled-three-router-topology`) | capability/evidence | plans/implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md | plans/closure/anonymity/297-status.md |
| 298 | stopped (`stopped-three-family-hostile-destination-capture-runners-unavailable`) | infrastructure/evidence | plans/implementation/anonymity/298-streaming-active-fingerprint-differential-harness.md | plans/closure/anonymity/298-status.md |
| 299 | stopped (`stopped-no-plan-298-differential-evidence`) | capability/corrective | plans/implementation/anonymity/299-streaming-observable-profile-convergence.md | plans/closure/anonymity/299-status.md |
| 300 | stopped (`stopped-target-isolation-and-reference-diversity-owner-gap`) | invariant/capability/evidence | plans/implementation/anonymity/300-destination-isolation-and-tunnel-path-anonymity-qualification.md | plans/closure/anonymity/300-status.md |
| 301 | stopped (`stopped-predecessor-qualification-evidence-incomplete`) | evidence/closure/polish | plans/implementation/anonymity/301-integrated-anonymity-evidence-and-scoped-claim-gate.md | plans/closure/anonymity/301-status.md |

## 8. Cross-cutting requirements

- No router-synthesized service-boundary bytes may disclose i2pr product/release/build identity or user-local naming state.
- Resolved Destination identity, not caller alias text, governs remote B32 presentation.
- Dedicated Destination identity remains default; sharing is explicit and linkable.
- Do not weaken NetDB namespace/provenance separation or tunnel-routed client lookup/publication.
- Do not weaken runtime budgets, cancellation, task ownership, secret redaction, or dependency direction.
- Differential evidence uses sanitized metadata/counts/hashes only; never raw payloads/private identities.
- Reference behavior is measured on exact pins before it becomes a compatibility target.
- No production tuning is justified solely by source-code constants.
- Randomization is not authorized by this roadmap.
- Protocol-required RouterInfo fields remain truthful.

## 9. Verification strategy

Plan 296 establishes transcript-negative/static checks. Plan 297 adds pinned HTTP differential captures. Plan 298 adds hostile-Destination Streaming stimuli and sanitized traces. Plan 300 adds deterministic identity/path-diversity fixtures plus composed client lookup/publication assertions. Plan 301 composes retained evidence and checks documentation claims.

External lanes are environment-gated, exact-pin/source-locked, fail-closed, and emit only sanitized committed evidence. Missing reference routers are unexecuted, never a pass. Routine workspace verification remains mandatory.

## 10. Risks and decision points

The principal risk is replacing a literal version leak with a subtler unique fingerprint. A custom user agent, unusual missing-header set, random serialization, unique Streaming constant combination, or nonstandard path profile can all partition the anonymity set.

A second risk is overclaiming: an HTTP proxy can remove router-added identifiers but cannot hide a browser/TLS/application fingerprint carried through an opaque stream.

A third risk is availability pressure silently weakening isolation/path diversity. Any fallback that violates a registered anonymity floor is explicit, bounded, locally observable, and excluded from the qualified profile.

Reference drift requires an explicit reconciliation pass rather than silent retuning.

## 11. Completion definition

This workstream is complete only when direct i2pr branding/local alias leakage is absent from qualified remote transcripts; HTTP's default privacy profile is reference-derived and differentially qualified; Streaming active-probe behavior has a retained baseline and selected convergent profile; Destination sharing/target isolation and tunnel/path diversity satisfy registered gates; NetDB namespace/tunneled client behavior remains protected; a machine-readable matrix distinguishes proven/unproven/out-of-scope dimensions; and docs/security-model.md states only the scoped properties justified by executed evidence.

No broad production-anonymity, browser-anonymity, or global-traffic-analysis claim is introduced.

## 12. Milestone status summary

Plan 296 is closed. Plan 297 is stopped until controlled Java/i2pd HTTP proxy captures exist. Plan 298 has bounded local schema/scenario fixtures but is stopped because hostile-Destination three-family runners/captures are absent; Plan 299 therefore remains stopped without differential evidence. Plan 300 implemented repeated-router rejection and a source-derived three-hop service profile, then stopped because multi-target HTTP/SOCKS identity isolation and candidate-diversity/degraded-path ownership are unresolved. Plan 301 is stopped until Plans 297, 299, and 300 pass. This workstream does not gate M12 and authorizes no broad anonymity claim.
