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

No public-network transit in Plans 249-262; no floodfill (M12); no new ElGamal generation;
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
- Plan 255 is retained as useful qualification scaffolding, but its completion interpretation
  is narrowed by post-closure source audit: one generic build can fan out into unrelated success
  rows, the Participant topology lacks i2pd-B, RouterInfo bootstrap is not proven against the
  active reference NetDB before selection, the transit responder key is unrelated to the
  advertised RouterIdentity encryption key, and rejection/replay/expiry/cancel/restart are not
  real external experiments.
- Plan 256/257 are retained as documented. Plan 258's H3 fragmentation correction is retained and externally proven. Plan 259's nine-chain inventory and transit-endpoint dispatch findings are retained, but its global receipt-is-OBEP-only conclusion is narrowed because it did not model the creator-owned `InboundTunnel` pool path. Plan 260 is retained-blocked: its seven creator-owned source locks, explicit-peer lock, fragment-id hardening, tuple validator, and receipt harness all landed with local rows green, and the dedicated receiver 1-hop `[i2pr]` inbound was exhibited live (four `[A,A]` IBGW accepts + SAM STATUS OK on a fresh mesh) — but delivery stops on two exact boundaries: B2 forward-path garlic death at B's endpoint (zero ingress on counted ids over four multicell rounds) and B1 late-run mesh sustainability (establishment 1/3, outbound collapse without floodfill). Plan 261 is retained-blocked: its B-sender lane work all landed with local rows green (four B-side source locks, B SAM plumbing with fail-closed env gate, `m11-tx-b` sender shape, terminal-signature instrumentation), and live execution proved the B-sender topology through addressing (B outbound `[i2pr]` established, B-side LeaseSet resolved from the floodfill store, all four sends naming counted `[A,A]` ids) — but delivery stops on one exact boundary: B3 self-delivery loopback gap (self-targeted OBEP TUNNEL actions terminate `NoActiveSession`, zero ingress, zero socket receipt). Plan 262 is retained-blocked: its dedicated IBGW state, exact receive-id ownership, source-neutral seam, and self-delivery loopback arm all landed with local rows green (three source locks, 5 + 4 + 7 regressions, 171 guarded checker rows), and live execution flipped B3 with socket receipt on a healthy mesh (diag4 `terminal-garlic-self:0/ingress:6/socket:1` with tuple-bound multicell on `514bf12`) — but two same-SHA full-matrix attempts stop on mesh-sustainability signatures (IBGW-data A-via-B relay zero ingress; SAM read timeout). Plan 263 is retained-blocked: its harness-only sustainability proofs (mesh-liveness, relay-NetDB, B-floodfill, canonical SAM tail, 4 harness rows, zero production diff) all landed with local rows green, and live execution re-proved receipt (`0/18/1` tuple-bound) and IBGW multicell (max 2) on `9bd2f39a` — but two same-SHA single-mesh full-matrix attempts stop on sustainability signatures (single-cell-only window; receipt starvation). Plan 264 is retained-blocked: its per-epoch lane + composition gate + manifest v4 + 5 composition rows all landed with local rows green (zero production diff), and live execution proved the gate (5 epochs close 2/2 on `6ab9dc2d`; `--compose` counts exactly those passes) — but emission epochs stop on window signatures (ibgw-data 1/2 single-cell-only; receipt 0/2 starvation; participant-data 1/2 no-forward; replay 0/2 setup stops; lifecycle structurally blocked; replay singleton contained with B-side proof, no wire duplicate). Plan 265 executed the fixed-budget opportunity-qualified contract it had frozen: manifest v5, a closed 16-token terminal vocabulary, three input-side opportunity predicates with a static output-side guard, explicit replay outcome kinds, and a `--compose-265` composer with 13 fixtures, all with zero production source diff; 24 retained attempts on one qualification SHA `4682920e` with zero i2pr semantic failures, closing `ibgw-data` 4/8 and `participant-lifecycle` 5/8 while `receipt` reached 1/8 against a required 2, so Plan 265 is retained-blocked. Plan 266 executed the ladder contract it had frozen (ten-rung input-side opportunity ladder, one new declared no-opportunity terminal with vocabulary 16 → 17, sanitized ladder rows, `--compose-265 --receipt-only` with 8 new fixtures, all with zero production source diff): 8 retained `receipt` attempts on one qualification SHA `a9803ca` with zero i2pr semantic failures, closing `receipt` 1/8 against a required 2 with rung distribution r1x2/r4x2/r6x3/r7x1 and rung 5 unobservable throughout, so Plan 266 is retained-blocked with every rung-6 attempt an anchored accepted-id drop population. Plan 267 is registered as the narrow accepted-id drop-disposition corrective. Ordinary product construction
  remains transit-disabled; its SSU2 pump consults only the disabled probe.

Current I2NP API 0.9.65 defines m/r/l/b bandwidth parameters. API 0.9.68+ requires tunnel
testing for routers advertising that protocol level. No M11 plan changes router.version or
advertises public transit support.

Plan 249 remains retained historical work corrected by Plan 250. Plan 251 repaired the
ordinary CI/source-lock boundary. Plan 255 qualification infrastructure is retained after the
post-closure audit. Repeated Plan 255 hosted dispatches
do not count until the evidence/topology defects are corrected. Plan 259 is retained with its topology inventory but corrected by Plan 260 on endpoint ownership; Plan 260 is retained-blocked with the creator-owned build half proven and B1/B2 delivery boundaries localized; Plan 261 is retained-blocked with the B-sender topology proven through counted-id addressing and the B3 self-delivery boundary localized; Plan 262 is retained-blocked with IBGW ownership corrected and receipt proven live (diag4 `0/6/1` on `514bf12`), full matrix stopped on sustainability; Plan 263 is retained-blocked with the sustainability harness landed and receipt (`0/18/1`) + multicell (max 2) re-proven on `9bd2f39a`, single-mesh two-pass stopped on sustainability; Plan 265 is the next executable M11 plan: implement manifest v5, input-side opportunity markers and replay outcome typing, then execute exactly 8 retained fresh-mesh attempts for each of `ibgw-data`, `receipt`, and `participant-lifecycle` on one qualification SHA. Any opportunity-present i2pr semantic contradiction hard-fails; the five deterministic Plan 264 epochs remain retained 2/2 because production source is byte-identical. M12 stays deferred.

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

Ordinary product profiles remain disabled. Plan 254 retains the controlled real-SSU2 owner
boundary; Plan 256 owns correction of the external qualification topology/evidence around that
boundary. No per-cell task spawning.

### Controlled external qualification

Plan 256 repairs the Plan 255 lane while retaining unmodified exact-pinned i2pd 2.61.0
(`635b013a612ff47278ef02acf8580a28e10e26c5`) as the independent oracle. Counted builds
must originate at stock i2pd, enter through actual authenticated SSU2 `next_inbound`, and
traverse an enabled controlled `TransitLiveOwner`. The responder private key must correspond
to the X25519 encryption public key in the exact signed i2pr RouterIdentity; the public RI must
be loaded by the exact reference NetDB before peer selection; role rows must derive from typed
decoded roles in separate epochs. Fabricated STBMs, direct registry insertion, patched
references, false RouterInfo claims, and public-network fallback do not count.

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
                                                    Plan 260 corrected exact-pinned i2pd qualification
                                                                  |
                                                                  v
                                                    Plan 261 B-sender receipt requalification
                                                                  |
                                                                  v
                                                    Plan 262 IBGW ingress ownership + self-delivery loopback corrective
                                                                  |
                                                                  v
                                                     Plan 263 qualification-sustainability corrective
                                                                   |
                                                                   v
                                                     Plan 264 single-mesh sustainability scoping
                                                                   |
                                                                   v
                                                     Plan 265 fixed-budget opportunity-qualified emission sustainability corrective
                                                                   |
                                                                   v
                                                     M11 experimental closure
                                                                 |
                                                                 v
                                                   M12 floodfill planning
~~~

Plans 250 and 251 are closed. Plan 252's full-message STBM core is retained. Plan 253
retains its bounded data-plane/envelope/rollback/drain/peer-state work; its live-owner
corrective is closed by passed Plan 254. Plan 255 qualification scaffolding is retained with
its post-closure defects recorded. Plan 256 is retained, Plan 257 is retained with its
reply/state/evidence repairs locally proven but externally stopped at the IBGW multicell
boundary, Plan 258 is retained with the emission correction proven, Plan 259's topology inventory/transit-endpoint findings are retained with its global receipt conclusion corrected via Plan 260, Plan 260 is retained-blocked with source locks, fragment-id hardening, tuple validator, and receipt harness landed plus the receiver 1-hop `[i2pr]` build exhibited live, stopped on the B2-forward/B1-sustainability delivery boundaries, Plan 261 is retained-blocked with the B-sender topology proven through counted-id addressing (B outbound `[i2pr]`, floodfill-store LeaseSet resolution, 4/4 sends naming counted ids), stopped on the B3 self-delivery loopback boundary, Plan 262 is retained-blocked with IBGW ownership corrected and receipt proven live (diag4 `0/6/1` on `514bf12`), stopped on the full-matrix sustainability boundary, Plan 263 is retained-blocked with the sustainability harness landed and receipt + multicell re-proven on `9bd2f39a`, stopped on the single-mesh sustainability boundary, Plan 264 is retained-blocked with the per-epoch lane + composition gate landed and 5 epochs closed 2/2 on `6ab9dc2d`, stopped on the per-epoch emission-window boundary, Plan 265 is retained-blocked after executing its frozen fixed-budget contract (24 retained attempts on `4682920e`, input-side opportunity classification, zero tolerated i2pr semantic contradictions, `ibgw-data` and `participant-lifecycle` closed, `receipt` 1/8), and Plan 266 owns receipt-family opportunity generation only.

## 7. Milestones

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 249 | retained | retained-m11-transit-foundation-corrected-via-plan250 | plans/implementation/transit-tunnels/249-m11-transit-admission-and-short-build-participant-foundation.md | plans/closure/transit-tunnels/249-status.md |
| 250 | closed (infrastructure only) | passed-m11-transit-foundation-semantic-and-ownership-corrective-infrastructure-only-m11-capability-not-claimed | plans/implementation/transit-tunnels/250-m11-transit-foundation-semantic-and-ownership-corrective.md | plans/closure/transit-tunnels/250-status.md |
| 251 | closed (cross-subsystem CI maintenance) | passed-java-source-lock-test-environment-gating-and-ordinary-ci-corrective | plans/implementation/mixed-router-interop/251-java-source-lock-test-environment-gating-and-ordinary-ci-corrective.md | plans/closure/mixed-router-interop/251-status.md |
| 252 | retained | retained-m11-daemon-transit-composition-corrective-required-via-plan253 | plans/implementation/transit-tunnels/252-m11-daemon-transit-composition.md | plans/closure/transit-tunnels/252-status.md |
| 253 | retained | retained-m11-live-daemon-transit-data-plane-corrective-required-via-plan254 | plans/implementation/transit-tunnels/253-m11-live-daemon-transit-data-plane-corrective.md | plans/closure/transit-tunnels/253-status.md |
| 254 | closed | passed-m11-live-ingress-body-threading-closure-corrective | plans/implementation/transit-tunnels/254-m11-live-ingress-body-threading-closure-corrective.md | plans/closure/transit-tunnels/254-status.md |
| 255 | retained | retained-m11-i2pd-qualification-infrastructure-evidence-topology-corrective-required-via-plan256 | plans/implementation/transit-tunnels/255-m11-exact-pinned-i2pd-transit-qualification.md | plans/closure/transit-tunnels/255-status.md |
| 256 | retained | retained-m11-i2pd-qualification-evidence-topology-corrective-required-via-plan257 | plans/implementation/transit-tunnels/256-m11-i2pd-qualification-evidence-topology-corrective.md | plans/closure/transit-tunnels/256-status.md |
| 257 | retained | retained-m11-production-self-reply-and-external-evidence-completion-corrective-required-via-plan258 | plans/implementation/transit-tunnels/257-m11-production-self-reply-and-external-evidence-completion-corrective.md | plans/closure/transit-tunnels/257-status.md |
| 258 | retained | retained-m11-ibgw-multicell-corrective-with-receipt-topology-boundary-required-via-plan259 | plans/implementation/transit-tunnels/258-m11-ibgw-data-plane-multicell-diagnostic-corrective.md | plans/closure/transit-tunnels/258-status.md |
| 259 | retained | retained-m11-ibgw-receipt-adjudication-endpoint-model-corrective-required-via-plan260 | plans/implementation/transit-tunnels/259-m11-ibgw-receipt-topology-adjudication-diagnostic.md | plans/closure/transit-tunnels/259-status.md |
| 260 | retained | retained-m11-creator-owned-inbound-partially-proven-forward-sustainability-boundary-corrective-required-via-plan261 | plans/implementation/transit-tunnels/260-m11-creator-owned-inbound-receipt-topology-and-planning-authority-corrective.md | plans/closure/transit-tunnels/260-status.md |
| 261 | retained | retained-m11-b-sender-topology-proven-self-delivery-boundary-corrective-required-via-plan262 | plans/implementation/transit-tunnels/261-m11-b-sender-receipt-requalification.md | plans/closure/transit-tunnels/261-status.md |
| 262 | retained | retained-m11-ibgw-ownership-corrected-receipt-proven-full-matrix-sustainability-boundary-corrective-required-via-plan263 | plans/implementation/transit-tunnels/262-m11-self-delivery-loopback-corrective.md | plans/closure/transit-tunnels/262-status.md |
| 263 | retained | retained-m11-sustainability-harness-landed-receipt-and-multicell-reproven-single-mesh-full-matrix-boundary-corrective-required-via-plan264 | plans/implementation/transit-tunnels/263-m11-qualification-sustainability-corrective.md | plans/closure/transit-tunnels/263-status.md |
| 264 | retained | retained-m11-per-epoch-gate-proven-emission-windows-bound-full-matrix-scoping-insufficient-corrective-required-via-plan265 | plans/implementation/transit-tunnels/264-m11-single-mesh-sustainability-scoping.md | plans/closure/transit-tunnels/264-status.md |
| 265 | retained | retained-m11-fixed-budget-opportunity-qualified-composition-proven-two-families-closed-receipt-opportunity-generation-below-frozen-bar-corrective-required-via-plan266 | plans/implementation/transit-tunnels/265-m11-per-epoch-emission-sustainability-corrective.md | plans/closure/transit-tunnels/265-status.md |
| 266 | retained | retained-m11-receipt-family-opportunity-ladder-landed-one-family-success-short-of-two-accepted-id-drop-scope-derived-corrective-required-via-plan267 | plans/implementation/transit-tunnels/266-m11-receipt-family-opportunity-generation-corrective.md | plans/closure/transit-tunnels/266-status.md |
| 267 | ready | registered-m11-receipt-family-accepted-id-drop-disposition-corrective-ready | plans/implementation/transit-tunnels/267-m11-receipt-family-accepted-id-drop-disposition-corrective.md | — |

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

### Plan 255 — retained qualification scaffold

Plan 255 landed useful surfaces: the ignored external Rust driver, loopback runner, exact-pin
checks, static evidence checker, and hosted workflow. Post-closure source audit narrowed the
completion claim. Its external row generator can mark many unrelated requirements true from one
generic observed build; role attribution is not typed; i2pd-B is not started for Participant;
the RI write is not proven against the running reference NetDB before selection; the
TransitHopMaterial responder key is generated independently of the advertised RouterIdentity;
and rejection/replay/expiry/cancel/restart rows are not backed by their claimed experiments.

The original local/full-workspace/CI evidence remains historical evidence for the scaffold. It
is not M11 external capability evidence and the Plan 255 workflow must not simply be dispatched
twice and counted.

### Plan 256 — qualification evidence/topology corrective

Plan 256 is retained corrective infrastructure. Its exact i2pd 2.61.0 / 635b013a... pin and corrected evidence/topology requirements remain inherited by Plan 260
and repairs the counted lane by requiring:

- RouterIdentity/build-responder X25519 key coherence;
- public RI installation into the exact source-locked reference NetDB before peer selection;
- separate typed OBEP, IBGW, and Participant epochs, with a real second i2pd reference for the
  intermediate Participant topology;
- typed append-only event evidence whose row predicates cannot fan out from a generic boolean;
- genuine Participant/OBEP/IBGW data-plane experiments;
- genuine code-30 rejection, replay, logical expiry, cancellation, session-close, and restart
  experiments;
- two complete fresh-datadir external passes on the same i2pr SHA.

M12 remains blocked until Plan 262 closes the corrected receipt-capable external matrix. Public transit and broad two-family conformance remain
separate.

### Plan 257 — production self-reply and external evidence completion corrective

Plan 257 implementation is retained corrective infrastructure after three same-SHA
external executions on 1784ff8 stopped at the systematic IBGW multicell boundary
(genuine single-cell-only gateway ingress, zero receipt, healthy sessions,
B-debug exonerated). It retains Plan 256's coherent RouterIdentity build key,
exact NetDB bootstrap, real i2pd-B, typed role epochs, anti-fan-out evidence,
and genuine OBEP/IBGW data work, and adds the locally proven closure
authority: exact-pinned source-lock/qualification for the local-IBGW versus
remote OBEP reply branches, the TransitLiveStateSnapshot seam, typed
bandwidth plumbing, the independent i2pd-B far-side framework, exact
cardinality helpers, full-drain/session-close/real-restart evidence shapes,
the macOS Clippy structural fix, and the two-attempt workflow gate. Its
external rows never execute because the driver aborts fail-closed at the
retained multicell gate; Plan 258 owns carrying them to green.

Plan 257 does not advertise transit or change product defaults. M12 remains blocked until a
closure record proves every acceptance criterion and the unblock audit explicitly advances the
ADR 0026 one-family experimental gate.

### Plan 258 — IBGW data-plane multicell diagnostic corrective

Plan 258 is retained. It delivered the gateway failure/nested-size
telemetry, classified H1/H2/H3 across four non-counted diagnostics
(H2 excluded, H3 confirmed), and landed the narrow production
emission correction (fragment-for-all-sizes path mirroring the
reference IBGW) with regressions and checker guards — the
multicell row passes externally on 38c939a. Receipt stops on the
recorded [i2pr→B-endpoint] topology boundary, owned by Plan 259.

### Plan 259 — retained topology inventory; endpoint-model corrective required

Plan 259's seven-run/nine-chain inventory and its source reading of
`TransitTunnelEndpoint(false)` remain retained. Its broader Fork-2 conclusion is not current
authority: exact-pinned i2pd has a distinct creator-owned `InboundTunnel` path that sets
`msg->from` to the pool-owned inbound tunnel before LOCAL garlic dispatch, allowing
`TunnelPool::ProcessGarlicMessage`. The historical A-ending chain did not bind its next tunnel
id to that receiver-owned local tunnel/pool, so it cannot retire the receipt row.

### Plan 260 — creator-owned inbound receipt topology and planning-authority corrective

Plan 260 is retained-blocked. It landed the full creator-owned inbound
source-lock set (seven rows), the explicit-peer inbound-selection lock,
the six-field receipt-tuple validator with endpoint-class-ordered
rejections, the `Epoch::IbgwReceipt` evidence shape, per-registration
and per-role fragmented IBGW message-id hardening with five
regressions, and the receipt-epoch harness with a non-counted
receipt-only diagnostic gate — all with local rows green and zero
regressions in the legacy matrix. Live execution exhibited the
dedicated receiver 1-hop `[i2pr]` inbound (four `[A,A]` IBGW accepts,
two with exact +1 cardinality, plus SAM STATUS OK on a fresh mesh),
then stopped on two exact boundaries with stop provenance: B2
forward-path garlic death at B's endpoint for A-side senders (zero
ingress on counted ids over four multicell-forcing rounds; B
reassembles type-11 fragments with no onward delivery) and B1
late-run mesh sustainability (receiver establishment 1/3; A's
outbound plane collapses within ~2 minutes with no floodfill). The
`[i2pr]`-sender diagnostic variant was statically refuted (no
data-plane self-loopback by design) and reverted. Plan 261 owns the
B-sender requalification.

### Plan 261 — B-sender receipt requalification

Plan 261 is retained-blocked. It landed the full B-sender lane
(four source locks: B SAM bridge surface, explicit-peer
outbound selection, outbound-endpoint TUNNEL-forward, B-side
LeaseSet resolution; B SAM plumbing with a fail-closed env
gate; the `m11-tx-b` sender shape; `b-sender-obep-accepted` /
`b-leaseset-resolved` / `b-sender-outcome` / `send-window-ms`
evidence; manifest `plan: 261`) with local rows green and zero
production diff. Live execution proved the topology through
addressing: receiver established with four counted `[A,A]`
IBGW registrations, B `m11-tx-b` established outbound
`[i2pr]`, B resolved A's LeaseSet from its floodfill local
store (store + found + added lines, zero lookup failures),
and all four 1400-byte sends named counted ids
(`0x2276888c` ×3, `0xda72a534` ×1) — then stopped on one
exact boundary with stop provenance: B3 self-delivery
loopback gap (each send decrypts as an OBEP TUNNEL action
with `target_router == self` and terminates
`NoActiveSession` at the session peer seam; 4/4 rounds plus
33 background self-terminals, zero ingress, zero socket
receipt). The receipt-first reorder and the full lifecycle
matrix were not executed (stop fired in the diagnostic
subset) and belong to Plan 262's qualification. Plan 262 owns
the bounded IBGW ingress ownership + self-delivery loopback corrective.

Pre-execution Plan 262 source review also found that current i2pr IBGW `TunnelGateway` processing inherits Participant-style build-creator `previous_peer` affinity, while exact-pinned i2pd dispatches gateway data by live tunnel id without creator-peer comparison. Plan 262 therefore owns that ingress-authorization correction and a source-neutral IBGW seam in addition to the B3 local loopback arm. Participant/OBEP previous-peer locks remain unchanged.

### Plan 262 — IBGW ingress ownership + self-delivery loopback corrective

Plan 262 is retained-blocked. It landed the full ownership
corrective (dedicated `TransitGatewayData`, exact receive-id
ownership via `expected_receive`, source-neutral
`route_ibgw_gateway` seam, OBEP TUNNEL-to-self local branch with
`LocalIbgwDelivered`/`LocalIbgwDropped` and no synthetic peer,
third-party + negative regressions, three exact-pinned source
locks, driver self-loop mapping, manifest `plan: 262`, 171
guarded checker rows) with local rows green (tunnel 396,
daemon 1257, workspace 3121) and zero capability/version change.
Live execution flipped B3 with socket receipt on a healthy mesh
(diag4 receipt-only, 103.9 s, fresh datadirs, same SHA `514bf12`:
B `m11-tx-b` established outbound `[i2pr]`, B LeaseSet resolved,
sends name counted A IBGW ids, `b-sender-outcome =
terminal-garlic-self:0/ingress:6/socket:1` with `gateway-receipt
= 1`, `gateway-receipt-once = true`, `multicell-bounded = true`,
`full-tuple-bound = true`). Two same-SHA complete full-matrix
attempts stop on mesh-sustainability signatures (attempt1 IBGW-
data A-via-B relay zero ingress, 745.9 s; attempt2 SAM read
timeout, 201.5 s; diags 1–3 bound the flake class). No row was
weakened; no tuning. Plan 263 owns the bounded harness
sustainability corrective with zero production diff.

### Plan 263 — qualification-sustainability corrective

Plan 263 is retained-blocked. It landed the full harness
corrective (mesh-liveness, relay-NetDB, B-floodfill
prerequisites, canonical SAM tail, 4 harness rows, manifest
`plan: 263`, checker invariants) with zero production
`crates/*/src` diff and local rows green (3125 workspace
tests), and live execution re-proved receipt (`terminal-
garlic-self:0/ingress:18/socket:1` tuple-bound, 257.6 s) and
IBGW multicell (17 ingresses, max 2) on `9bd2f39a` — but two
same-SHA single-mesh full-matrix attempts stop on
sustainability signatures (single-cell-only window, 400.3 s;
receipt starvation, 1047.4 s). No row weakened; no tuning.

### Plan 264 — single-mesh sustainability scoping

Plan 264 is retained-blocked. It landed the per-epoch lane
(fine-grained epoch gates, setup prerequisites, lifecycle
chain, manifest v4 with `epoch_qualification`,
composition script + 5 composition rows, workflow
epoch/pass inputs, checker rules) with zero production
`crates/*/src` diff and local rows green (3130 workspace
tests), and live execution proved the gate (5 epochs close
2/2: obep/ibgw/participant/reject/obep-data on `6ab9dc2d`;
`--compose` counts exactly those passes and fails closed
with the missing set) — but emission epochs stop on window
signatures (ibgw-data 1/2 single-cell-only; receipt 0/2
starvation; participant-data 1/2 no-forward; replay 0/2
setup stops; lifecycle structurally blocked). The replay
singleton (healthy setup + predicate failure with B-side
containment, no wire duplicate) is recorded LOW. No row
weakened; no tuning.

### Plan 265 — fixed-budget opportunity-qualified emission sustainability corrective

Plan 265 executed its counting shape, frozen before
execution. It retains every external dispatch and replaces outcome-driven
reruns with three fixed scenario families: `ibgw-data`, `receipt`, and
`participant-lifecycle`. Each family runs exactly 8 fresh meshes on one
qualification SHA. Opportunity is classified from an input-side predicate
before i2pr semantic output; any opportunity-present i2pr contradiction is
a hard failure. Successful closure requires at least two qualifying
successes per family and no semantic failures. The Participant family
counts forward + B far-side + replay/expiry/session-close/cancel/restart as
one same-attempt lifecycle chain, matching the already-existing execution
order rather than re-paying the same rare forward prerequisite six times.
All Plan 265 work remained test/harness/checker/workflow only; production
`crates/*/src` is identical to the Plan 262 implementation `514bf12`, which
the lane now re-proves mechanically on every run rather than asserting at
review. Execution closed `ibgw-data` 4/8 and `participant-lifecycle` 5/8 with
zero i2pr semantic failures, and left `receipt` at 1/8 against a required 2.
M11 remains non-advertised/unclaimed and ADR 0026 stays unpassed. M12
remains deferred until Plan 266 closes the `receipt` family.

### Plan 266 — receipt-family reference-completion opportunity generation corrective

Plan 266 is the registered narrow successor. It owns **qualification
opportunity generation only** for the `receipt` scenario family, per Plan 265
§14: Plan 262 production routing is not reopened, and the `ibgw-data` and
`participant-lifecycle` families stay closed on the Plan 265 evidence and are
not re-executed. Its scope is a ten-rung input-side receipt opportunity ladder
that names which upstream precondition was missing (instead of re-rolling all
three), one new declared no-opportunity token
`receipt-creator-leaseset-lost-ibgw-id`, a creator-advertised-IBGW-receive-id
comparison with an explicit unobservable fallback, a `--receipt-only`
composition mode that refuses a fresh set for the closed families, and exactly
eight retained `receipt` attempts on one qualification SHA. If that budget is
exhausted without two qualified successes, the distribution of
first-unsatisfied rungs across the eight retained attempts is the evidence and
the next scope must derive from it — this plan does not license a second
budget increase.

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
live-owner corrective completed by passed Plan 254. Plan 255 qualification scaffolding is
retained after post-closure evidence/topology audit. Plan 256 is retained after implementation review; Plan 257 is retained with reply/state/evidence repairs locally proven but stopped at the systematic IBGW multicell boundary; Plan 258 is retained with the emission correction proven and the receipt-topology boundary recorded; Plan 259 is retained with corrective required via Plan 260. Plan 260 is retained-blocked with the creator-owned build half proven and B1/B2 delivery boundaries localized, corrective required via Plan 261. Plan 261 is retained-blocked with the B-sender topology proven through counted-id addressing and the B3 self-delivery boundary localized, corrective required via Plan 262. Plan 262 is retained-blocked with IBGW ownership corrected and receipt proven live (diag4 `0/6/1` on `514bf12`), corrective required via Plan 263. Plan 263 is retained-blocked with the sustainability harness landed and receipt (`0/18/1`) + multicell (max 2) re-proven on `9bd2f39a`, corrective required via Plan 264. Plan 264 is retained-blocked with the per-epoch gate proven (5 epochs 2/2 on `6ab9dc2d`) and emission windows bound, corrective required via Plan 265. Plan 265 is retained-blocked: its 24 retained attempts on `4682920e` satisfied the frozen composition rule for two of three families with zero i2pr semantic contradictions, and the `receipt` family ended attempt 8 at 1 of 2, so corrective is required via Plan 266. M12 floodfill remains deferred until the `receipt` family closes and M11 closes without an i2pr semantic contradiction.
