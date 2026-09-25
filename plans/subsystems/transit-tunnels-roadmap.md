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

No public-network transit in Plans 249-255; no floodfill (M12); no new ElGamal generation;
no Proposal 153 data layer; no broad RouterInfo/router.version change in Plan 249; no Java
full-router gate for experimental M11 progression; no resource-governor bypass.

## 4. Current state

The runtime-neutral and controlled-owner substrate is substantially complete:

- Plan 250 owns corrected admission/reply/registry semantics.
- Plan 252 retains the canonical full-message STBM transaction and role metadata.
- Plan 253 retains the bounded transit data plane, complete envelopes, rollback/drain, and
  bounded peer index.
- Plan 254 provides the single-decode `TransitInboundBodies` handoff and controlled
  `TransitLiveOwner::handle_inbound` with real `Ssu2InboundI2np`, creator/service
  ownership ordering, OBEP delivery, IBGW ingress, and outer cancellation.
- Ordinary product construction remains transit-disabled. Its SSU2 pump consults only the
  disabled probe; Plan 255 must bind an enabled controlled owner to the real
  `Ssu2DaemonHandle::next_inbound()` stream before external results count.

Current I2NP API 0.9.65 defines m/r/l/b bandwidth parameters. API 0.9.68+ requires tunnel
testing for routers advertising that protocol level. No M11 plan changes router.version or
advertises public transit support.

Plan 249 remains retained historical work corrected by Plan 250. Plan 251 repaired the
ordinary CI/source-lock boundary. Plan 255 is the registered external qualification gate.

## 5. Target architecture

### Runtime-neutral transit core

i2pr-tunnel owns typed m/r/l/b interpretation, deterministic admission, a dedicated
TransitRegistry keyed by receive tunnel id, role-local secret state, exact expiry, and
transactional build postprocessing.

Plan 252 adds the full-message seam required before daemon composition. One runtime-neutral
operation must consume the complete count-prefixed STBM, locate/open the local record once,
reuse the Plan 250 admission/reply semantics, replace the local slot, ChaCha20-transform
every other slot exactly once with the same derived reply key and target-slot nonce, and
return the complete transformed payload plus non-secret role/routing metadata. Accepted
registration commits only after full-message construction succeeds; valid code-30 rejection
returns a transformed message with zero registration. Reply keys, LayerKeys, Noise state,
and decrypted request bytes do not cross into the daemon.

Admission inputs include enabled/degraded/shutdown state, global active/pending ceilings,
per-peer active/pending ceilings, available share bandwidth, and optional per-tunnel cap.

Admission occurs before live registration.

### Daemon/runtime composition

Plans 252-254 establish the full-message processor, role-correct daemon composition,
bounded data plane, and controlled `TransitLiveOwner`. Participant/IBGW forward the
already-transformed STBM; OBEP emits OTBRM from the same transformed record set. Existing
router delivery owns build/TunnelData next-hop delivery. The daemon must not reopen,
reseal, export reply keys, or run a second independent message processor.

Ordinary product profiles remain disabled. Plan 255 owns the controlled real-SSU2
composition used for qualification. No per-cell task spawning.

### Controlled external qualification

Plan 255 uses unmodified exact-pinned i2pd 2.61.0
(`635b013a612ff47278ef02acf8580a28e10e26c5`) as the independent oracle. Counted builds
originate at stock i2pd, enter through actual authenticated SSU2 `next_inbound`, and
traverse an enabled controlled `TransitLiveOwner`. Fabricated STBMs, direct registry
insertion, patched references, false RouterInfo claims, and public-network fallback do not
count.

The lane qualifies OBEP, IBGW, and intermediate Participant builds with real data-plane
traffic, code-30 rejection, truthful bandwidth-option disposition, logical expiry/cleanup,
and two complete same-SHA executions.

## 6. Dependency graph

~~~text
Plan 248
  -> Plan 249 runtime-neutral foundation attempt
      -> Plan 250 transit semantic/ownership corrective ----\
                                                       +--> Plan 252 daemon/runtime composition
Plan 251 Java source-lock CI corrective ---------------/         |
                                                                 v
                                                   Plan 253 live daemon/data-plane corrective
                                                                 |
                                                                 v
                                                   Plan 254 live ingress/body closure corrective
                                                                 |
                                                                 v
                                                   Plan 255 exact-pinned i2pd qualification
                                                                 |
                                                                 v
                                                   M11 experimental closure
                                                                 |
                                                                 v
                                                   M12 floodfill planning
~~~

Plans 250 and 251 are closed. Plan 252's full-message STBM core is retained. Plan 253
retains its bounded data-plane/envelope/rollback/drain/peer-state work; its live-owner
corrective is closed by passed Plan 254. Plan 255 exact-pinned i2pd controlled transit
qualification is registered ready and is the next executable M11 plan.

## 7. Milestones

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 249 | retained | retained-m11-transit-foundation-corrected-via-plan250 | plans/implementation/transit-tunnels/249-m11-transit-admission-and-short-build-participant-foundation.md | plans/closure/transit-tunnels/249-status.md |
| 250 | closed (infrastructure only) | passed-m11-transit-foundation-semantic-and-ownership-corrective-infrastructure-only-m11-capability-not-claimed | plans/implementation/transit-tunnels/250-m11-transit-foundation-semantic-and-ownership-corrective.md | plans/closure/transit-tunnels/250-status.md |
| 251 | closed (cross-subsystem CI maintenance) | passed-java-source-lock-test-environment-gating-and-ordinary-ci-corrective | plans/implementation/mixed-router-interop/251-java-source-lock-test-environment-gating-and-ordinary-ci-corrective.md | plans/closure/mixed-router-interop/251-status.md |
| 252 | retained | retained-m11-daemon-transit-composition-corrective-required-via-plan253 | plans/implementation/transit-tunnels/252-m11-daemon-transit-composition.md | plans/closure/transit-tunnels/252-status.md |
| 253 | retained | retained-m11-live-daemon-transit-data-plane-corrective-required-via-plan254 | plans/implementation/transit-tunnels/253-m11-live-daemon-transit-data-plane-corrective.md | plans/closure/transit-tunnels/253-status.md |
| 254 | closed | passed-m11-live-ingress-body-threading-closure-corrective | plans/implementation/transit-tunnels/254-m11-live-ingress-body-threading-closure-corrective.md | plans/closure/transit-tunnels/254-status.md |
| 255 | ready | registered-m11-exact-pinned-i2pd-transit-qualification-ready | plans/implementation/transit-tunnels/255-m11-exact-pinned-i2pd-transit-qualification.md | pending |

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

First prove the full-message short-build seam: complete STBM validation, unique local slot,
one request open/KDF, exact Plan-250 own reply, one ChaCha20 transform of every other slot,
commit-after-full-message construction, code-30 transformed rejection with zero state, and
typed Participant/IBGW/OBEP routing metadata. Then prove bounded daemon queue admission,
authenticated previous-peer handoff, Participant/IBGW STBM forwarding, OBEP OTBRM
termination, TunnelData next-hop dispatch, expiry/cancellation/shutdown cleanup, creator
correlation, and no per-cell task explosion.

### Plan 253

Correct the daemon/data-plane completion boundary: wire the controlled gate into the live
authenticated SSU2/router-I2NP owner, replace the no-op TunnelData placeholder with
canonical role/replay/expiry processing, preserve and deliver code-30 routes, construct
complete I2NP build messages, roll back every terminal delivery failure, drain secrets on
shutdown, and enforce bounded peer/session state.

### Plan 254

Closed the controlled live-owner/body-threading boundary: canonical single-decode handoff,
`TransitLiveOwner::handle_inbound`, outer cancellation/session lifecycle,
creator/service-versus-transit ownership, OBEP delivery, IBGW ingress, and green exact-SHA
CI. Ordinary product pump behavior remains disabled/probe-only; Plan 255 must prove the
enabled owner on actual SSU2 `next_inbound`.

### Plan 255

Against unmodified exact-pinned i2pd 2.61.0, first prove real authenticated
`Ssu2DaemonHandle::next_inbound()` events traverse an enabled `TransitLiveOwner`. Then
prove genuine i2pd-created OBEP, IBGW, and intermediate Participant builds, accepted
replies, role-correct TunnelData/TunnelGateway traffic, code-30 rejection, truthful
bandwidth-option disposition, logical expiry/replay/cleanup, and two complete same-SHA
executions.

If deterministic role placement needs patched i2pd or false RouterInfo claims, stop and
register a narrow topology successor.

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
admission/registration contract, a genuinely live bounded daemon/data-plane composition,
and exact-pinned i2pd controlled build/forward/expiry/cleanup evidence. Public-network enablement stays separately gated.

Full two-family router conformance is not required to begin M12 development under ADR
0026, but remains required for a broad full-conformance/advertisement claim.

## 12. Milestone status summary

Plan 249 is retained with findings corrected by closed Plan 250. Plan 251 closed the
ordinary-CI/source-lock corrective. Plan 252 retains the full-message STBM core. Plan 253
retains the runtime-neutral data-plane/envelope/rollback/drain/bounded-peer work, with its
live-owner corrective completed by passed Plan 254. Plan 255 exact-pinned i2pd controlled
transit qualification is registered ready and is the next executable plan. M12 floodfill
remains deferred until Plan 255 supplies controlled transit/resource evidence.
