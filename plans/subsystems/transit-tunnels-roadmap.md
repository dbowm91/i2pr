# Transit Tunnels Roadmap

Status: active

Long-term references:

- GUARDRAILS.md
- specs/CONFORMANCE.md
- specs/support.toml
- specs/protocols/05-tunnels.md
- specs/protocols/02-i2np.md

Related ADRs:

- docs/adr/0026-staged-interoperability-progression-and-java-debt.md

Normative external references:

- https://www.i2p.net/en/docs/specs/tunnel-creation-ecies/
- https://i2p.net/en/docs/specs/i2np/
- https://www.i2p.net/en/docs/specs/tunnel-implementation/
- https://i2p.net/en/proposals/168-tunnel-bandwidth/

Exact-pinned behavioral references remain those in specs/SOURCES.md: i2pd 2.61.0 and
Java I2P 2.13.0. They are behavior references, not code to copy.

## 1. Purpose and ownership boundary

M11 turns existing local participant primitives into a bounded router service that can
accept a current ECIES short-build request from another router, decide whether to
participate, install only hop-local state, forward TunnelData for the accepted lifetime,
and remove state deterministically.

This subsystem owns transit participation only. It does not own creator tunnel pools,
destination/client tunnels, floodfill, or public-network exposure.

A transit hop may know only its own receive id, next router/tunnel tuple, hop-local keys,
role, previous peer, expiry, and bounded accounting state. It must not gain creator path
knowledge.

## 2. Work classification

- **Capability** — controlled mixed-router transit participation after final M11
  qualification.
- **Infrastructure** — typed bandwidth options, admission, transit registry,
  build-response construction, daemon queue/dispatch.
- **Invariant** — admission before allocation, previous-peer lock, replay suppression,
  exact expiry, secret ownership, truthful non-advertisement.
- **Polish** — metrics/evidence/docs after correctness.

## 3. Non-goals

No public-network transit in Plans 249-253; no floodfill (M12); no new ElGamal generation;
no Proposal 153 data layer; no broad RouterInfo/router.version change in Plan 249; no Java
full-router gate for experimental M11 progression; no resource-governor bypass.

## 4. Current state

The cryptographic/data-plane substrate is ahead of runtime service:

- short_record.rs implements current request/reply layouts, roles, 600-second lifetime,
  AES layer type, and response codes 0/30.
- build_crypto.rs / responder.rs can open a request, derive hop keys, and seal a reply.
- roles.rs implements participant forward transforms, previous-peer locking, replay
  rejection, and gateway/endpoint primitives.
- data_plane_registry.rs is bounded creator/local-pool state, not a transit registry.
- router_i2np.rs decodes authenticated ShortTunnelBuild but intentionally returns
  TunnelBuildReserved. This is the first production composition gap.
- TunnelData already has an authenticated-router typed dispatch seam.

Current I2NP API 0.9.65 defines m/r/l/b tunnel bandwidth parameters. API 0.9.68+ also
requires tunnel testing for routers advertising that protocol level. No M11 foundation
plan alters router.version or advertises transit support.

Plan 249 landed the intended runtime-neutral architecture and remains retained historical
work. Plan 250 corrected its provenance, wire-reply, time-window, pending-reservation,
secret-ownership, panic-safety, and direct-test defects. Plan 251 independently repaired the
ordinary CI/source-lock boundary.

## 5. Target architecture

### Runtime-neutral transit core

i2pr-tunnel owns typed m/r/l/b interpretation, deterministic admission, a dedicated
TransitRegistry keyed by receive tunnel id, role-local secret state, exact expiry, and
transactional build postprocessing.

Admission inputs include enabled/degraded/shutdown state, global active/pending ceilings,
per-peer active/pending ceilings, available share bandwidth, and optional per-tunnel cap.

Admission occurs before live registration.

### Daemon/runtime composition

A later plan replaces ShortTunnelBuild's reserved outcome with bounded supervised
composition, uses existing router delivery for build forwarding/replies and TunnelData
next-hop delivery, and removes transit state on expiry/shutdown. No per-cell task spawning.

### Controlled external qualification

The final M11 lane uses exact-pinned i2pd as the primary independent router oracle and
requires genuine short-build and TunnelData traffic, not injected registry state.

## 6. Dependency graph

~~~text
Plan 248
  -> Plan 249 runtime-neutral foundation attempt
      -> Plan 250 transit semantic/ownership corrective ----\
                                                       +--> Plan 252 daemon/runtime composition
Plan 251 Java source-lock CI corrective ---------------/         |
                                                                 v
                                                   Plan 253 exact-pinned i2pd qualification
                                                                 |
                                                                 v
                                                   M11 experimental closure
                                                                 |
                                                                 v
                                                   M12 floodfill planning
~~~

Plans 250 and 251 are closed. Plan 252 remains unregistered until ordinary CI is green on
the Plan 250 implementation SHA; after that audit it is the next executable plan. Plan 253
remains unregistered until Plan 252 closes.

## 7. Milestones

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 249 | retained | retained-m11-transit-foundation-corrected-via-plan250 | plans/implementation/transit-tunnels/249-m11-transit-admission-and-short-build-participant-foundation.md | plans/closure/transit-tunnels/249-status.md |
| 250 | closed (infrastructure only) | passed-m11-transit-foundation-semantic-and-ownership-corrective-infrastructure-only-m11-capability-not-claimed | plans/implementation/transit-tunnels/250-m11-transit-foundation-semantic-and-ownership-corrective.md | plans/closure/transit-tunnels/250-status.md |
| 251 | closed (cross-subsystem CI maintenance) | passed-java-source-lock-test-environment-gating-and-ordinary-ci-corrective | plans/implementation/mixed-router-interop/251-java-source-lock-test-environment-gating-and-ordinary-ci-corrective.md | plans/closure/mixed-router-interop/251-status.md |
| 252 | planned | unregistered-pending-plan250-ordinary-ci-green | not yet written | not yet written |
| 253 | planned | unregistered-after-plan252 | not yet written | not yet written |

## 8. Cross-cutting requirements

- Authenticate/decode before policy use; do not allocate long-lived role state before
  admission succeeds.
- Bound build work and active transit globally/per-peer.
- Caller-supplied time in runtime-neutral tests.
- Duplicate receive ids fail closed; unexpired state is never silently evicted.
- Keep keys/decrypted records out of Debug/logs.
- Preserve previous-peer lock and duplicate-token protection.
- Clean state on reject, failed commit, expiry, cancellation/shutdown, and owner-defined
  terminal failures.
- Transit traffic remains lower priority than router-owned/client traffic and must obey
  router-wide resource governance.
- m/r/l/b are positive integer KBps; m <= r <= l; l is IBGW-only; accepted m/r replies
  should include b >= m.
- Current ECIES replies intentionally expose only 0 (accept) and 30 (bandwidth reject) to
  reduce fingerprinting. Keep local rejection taxonomy internal; any well-formed request
  rejected by admission policy maps to 30. Malformed/unauthenticated input fails closed.

## 9. Verification strategy

### Plan 249

Deterministic local tests cover legal/illegal m/r/l forms, integer/ordering errors, role
restriction for l, disabled/degraded/capacity/per-peer limits, insufficient bandwidth ->
code 30, accepted b semantics, duplicate receive ids, rollback, role registration,
virtual-time expiry, previous-peer/replay regressions, and secret redaction.

### Plan 250

Correct the runtime-neutral contract: authenticated previous-peer provenance, actual sealed
reply b/30 semantics, creation/expiry direction, real pending reservations, move-only
secret owners, panic-free removal, and direct requirement tests.

### Plan 251

Restore ordinary CI by explicitly environment-gating only exact-pinned Java source-tree
source-lock tests. This cross-subsystem maintenance does not reopen M6 progression.

### Plan 252

After Plans 250/251 close and CI is green, prove bounded daemon queue admission,
cancellation/shutdown cleanup, authenticated previous-peer handoff, next-hop dispatch,
expiry, and no per-cell task explosion.

### Plan 253

Against exact-pinned i2pd prove genuine build addressed to i2pr, accepted encrypted reply,
TunnelData receive-id dispatch and next-id forwarding, digest-bound delivery, bandwidth
rejection, expiry/duplicate failure, repeated exact-head stability, and cleanup baseline.

Qualify participant plus edge roles before broad M11 closure where the controlled topology
can exercise them. Otherwise narrow the claim explicitly.

## 10. Risks and decision points

BuildOptions is currently an opaque validated Mapping; typed interpretation must layer over
it without weakening the codec. The admission transaction must not commit secrets before
resource acceptance and successful response construction. Transit is remote-selected work,
so active counts, crypto, bandwidth, and queues require explicit ceilings.

Do not change router.version/caps from a local infrastructure pass. If exact-pinned i2pd
shows mandatory legacy build behavior for the intended current peer subset, stop and
register a separate compatibility plan.

## 11. Completion definition

M11 experimental progression closes only after the corrected runtime-neutral
admission/registration contract, bounded daemon composition, and exact-pinned i2pd
controlled build/forward/expiry/cleanup evidence. Public-network enablement stays separately gated.

Full two-family router conformance is not required to begin M12 development under ADR
0026, but remains required for a broad full-conformance/advertisement claim.

## 12. Milestone status summary

Plan 249 is retained with its corrective findings addressed by closed Plan 250. Plan 251
closed the ordinary-CI/source-lock corrective with CI green on `6cf441d`. Plan 252 is the
next M11 scope but remains unregistered until ordinary CI is green on the Plan 250
implementation SHA. Plan 253 remains unregistered behind Plan 252. M12 floodfill remains
deferred until M11 controlled transit/resource evidence exists.
