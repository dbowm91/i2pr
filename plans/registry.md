# i2pr Active Planning Registry

Compact control surface for active planning. Detailed requirements and completed history
remain in subsystem roadmaps, implementation plans, `plans/closure/`, and Git history.
Projected from the frozen `plans/archive/legacy-flat-registry-2026-09-18.md` on 2026-09-18;
the i2pr token in each closure record stays authoritative, `state` is the codegg projection.

Canonical direction remains in `GUARDRAILS.md`, `specs/CONFORMANCE.md`,
`specs/support.toml`, and `docs/adr/`.

## Status vocabulary projection

| i2pr token shape | Codegg state | Meaning |
|---|---|---|
| `passed-*` | `closed` | Closure record accepted; evidence landed |
| `retained-*` (no `superseded-by`) | `retained` | Evidence kept; interpretation narrowed |
| `*-superseded-by-planNNN` | `superseded` | Replaced; see the superseding plan |
| `blocked-*`, `stopped-*` | `blocked` | Named dependency/evidence gap prevents progress |
| `in-progress-*` | `active` | Implementation or closure work in progress |
| `registered-*`, `pending`, `next *` | `ready` | Approved for execution when dependencies clear |
| `skipped`, `retired*` | `archived` | Never authoritative; retained for traceability |

## Active subsystem roadmaps

| Subsystem | Status | Roadmap | Current milestone | Dependencies or blockers |
|---|---|---|---|---|
| Workspace foundation | closed | `plans/subsystems/workspace-foundation-roadmap.md` | Plans 000–025 closed | — |
| NTCP2 transport | closed | `plans/subsystems/ntcp2-transport-roadmap.md` | Plans 030–101 exited (defect localized, daemon NTCP2 disabled) | New NTCP2 work needs a new plan-of-record |
| NetDB | closed | `plans/subsystems/netdb-roadmap.md` | Plans 102–106 closed (handoff via Plan 106) | — |
| Exploratory tunnels | closed | `plans/subsystems/exploratory-tunnels-roadmap.md` | Plans 107–117 closed (Plan 117 live integration) | — |
| Destinations / Garlic / Streaming (local) | closed | `plans/subsystems/destination-streaming-roadmap.md` | Plan 134 local authority; Plan 152 corrective retained | Mixed-router interop explicitly not claimed |
| SAM 3.1 | closed | `plans/subsystems/sam-roadmap.md` | Plan 151 final acceptance | Loopback-only, disabled by default |
| SSU2 transport | closed | `plans/subsystems/ssu2-roadmap.md` | Plans 161 + 162 closed | Classical X25519 only; no ML-KEM |
| I2CP | closed | `plans/subsystems/i2cp-roadmap.md` | Plan 172 final acceptance (experimental, loopback-only) | No `HostLookup`/`HostReply` |
| Service tunnels | active | `plans/subsystems/service-tunnels-roadmap.md` | Plan 215 passed; Plan 204 convergence open | M10 product authority is closed; Plan 204 convergence waits on independent M6 Java second-family closure. Plan 232 is the active M6-only lease-gateway fixture corrective and does not reopen M10 |
| M6 mixed-router interop | active | `plans/subsystems/mixed-router-interop-roadmap.md` | Plan 232 ready: route-derived lease-gateway fixture corrective + Java second-family continuation | Plan 231 proved the local LS2 advertises Router B while the installed inbound gateway is Router A. Plan 232 derives all three Java-driver local leases from the installed route, then reruns raw Destination and, on pass, Streaming. |

## Current milestone authorities

- **M6 local**: Plan 134 (`passed-milestone6-recv-window-ack-ceiling-closure`) —
  `plans/closure/destination-streaming/134-status.md`.
- **M7 SAM 3.1**: Plan 151 (`passed-m7-sam31-final-acceptance-evidence-correction`) —
  `plans/closure/sam/151-status.md`. Plan 152 is the retained M6 robustness corrective.
- **M8 SSU2**: Plan 161 (independent interop + final closure) + Plan 162 (lane isolation) —
  `plans/closure/ssu2/161-status.md`, `162-status.md`.
- **M9 I2CP**: Plan 172 (independent LeaseSet2 lifecycle corrective; final acceptance) —
  `plans/closure/i2cp/172-status.md`. Plan 170 wire/data-plane retained-passed.
- **M10 service tunnels**: Plan 215 (hosted Plan 214 re-verification) —
  `plans/closure/service-tunnels/215-status.md`; Plan 214 product closure; Plan 213 generic
  external qualification (`P213-N-passed` twice on exact commit `ef59fb3`).
- **M6 mixed-router program**: Plan 217 closed the harness corrective; Plan 218 retains the reverse-delivery behavioral stop; Plan 219 attribution is superseded; Plan 220 refuted J219-B; Plan 221 is superseded-before-execution; Plan 222 narrowed OCMOSJ to status 17; Plan 223 corrected Destination/LS2 separation and moved the tracked send to `ACCEPTED -> NO_LEASESET (21)`; Plan 224 closed with `P224-OBSERVABILITY-GAP-LOOKUP-PATH`; Plan 225 closed with exact `P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B`; Plan 226 closed with exact `P226-BASELINE-B-ZERO-HOP-UNKNOWN` and did not admit topology correction; Plan 227 closed with `P227-EXPLICIT-ONE-HOP-NOT-BUILT` (Router C selectable on all 3 counted attempts, no one-hop tunnels built within the five-minute ceiling); Plan 228 closed with `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both` (client configs through C proven, paired tunnel unavailable in both directions on all 4 counted attempts); Plan 229 closed with `P229-C-NOT-EXPLORATORY-ELIGIBLE` (roles + A-only small-router profile proven live, tier population empty on all 4 counted attempts on `00dc368`/`10d1015`); Plan 230 closed with `P230` reverse-delivery boundary (predicate proven, stock C1+C2 correction, natural bootstrap 2/3, forward delivery digest-matched once, reverse admitted-but-unarrived on `09a2f6e`/`3ef5bac`). Plan 231 closed with `P231-C-TARGET-IBGW-NOT-INSTALLED` (post-`ACCEPTED` attribution complete on `1f5a495`: A enqueue proven, C OBEP processing proven, exact target IBGW on B proven absent twice, i2pr wire counters honestly zero; root-caused to the test-driver lease fixture advertising gateway B vs actual inbound gateway A; no production change). Plan 232 is registered-ready to derive all local LS2 lease gateways/tunnel ids from the installed inbound route and continue through raw-Destination + Streaming qualification. M6 Java remains not-yet-passed.

## Dependency-ready and active plans

| Subsystem | Plan | State | Handoff | Dependencies / handoff note |
|---|---|---|---|---|
| M6 mixed-router interop | 232 route-derived lease-gateway fixture corrective and second-family closure | ready | `plans/implementation/mixed-router-interop/232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure.md` | Correct all three Java-driver local LS2 lease sites from the installed inbound route; preserve Router B as publication target; rerun raw Destination and continue directly to Streaming if reverse delivery passes. |

## Blocked work

| Subsystem | Plan | Blocker |
|---|---|---|
| M6 mixed-router interop | 201 | Blocked pending Plan 232 route-derived lease-gateway fixture corrective and Java second-family continuation |
| Service tunnels | 204 | Cross-milestone convergence waits for M6 Java second-family closure, now pending Plan 232; closed M10 product authority unchanged |
| M6 mixed-router interop | 187 / 188 / 191 (historical) | Retained `blocked`/`stopped` tokens; rows partially flipped by Plans 190/192/193 — see roadmap |

### Retained / conditional work

- **Plan 205 SAM/helper pivot** — `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`. Plan 229 remains on the direct stock-Java exploratory/client tunnel build path below SAM, so no SAM pivot is authorized.

## Recently closed work

| Subsystem | Plan | State | Closure record |
|---|---|---|---|
| M6 mixed-router interop | 231 reverse-delivery tunnel-dispatch attribution corrective | closed | `plans/closure/mixed-router-interop/231-status.md` (`passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary`; post-`ACCEPTED` attribution complete on `1f5a495`: A enqueue proven, C OBEP processing proven, exact target IBGW on B proven absent twice, i2pr wire honestly zero; root-caused to the test-driver lease fixture; no production change) |
| M6 mixed-router interop | 230 reachability-capability/profile-bootstrap corrective + continuation | closed | `plans/closure/mixed-router-interop/230-status.md` (`passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary`; baseline compound-ineligible on `09a2f6e`, C1+C2 correction on `3ef5bac`, 3/3 eligible, natural bootstrap 2/3, forward delivery digest-matched once, reverse admitted-but-unarrived) |
| M6 mixed-router interop | 229 non-zero exploratory paired-tunnel bootstrap corrective | closed | `plans/closure/mixed-router-interop/229-status.md` (`passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary`; roles + A-only small-router profile proven live, `P229-C-NOT-EXPLORATORY-ELIGIBLE` on all 4 counted attempts on `00dc368`/`10d1015`; revised Plan 230 registered for the reachability-capability/profile-bootstrap corrective) |
| M6 mixed-router interop | 228 client-tunnel build-path attribution | closed | `plans/closure/mixed-router-interop/228-status.md` (`passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary`; `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both` on all 4 counted attempts on `8f2167d`/`8990849`) |
| M6 mixed-router interop | 227 explicit one-hop with selectable-C-but-not-built | closed | `plans/closure/mixed-router-interop/227-status.md` (`passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary`; `P227-EXPLICIT-ONE-HOP-NOT-BUILT` on all 3 counted attempts on `b59bf5b`, C selectable) |
| M6 mixed-router interop | 223 identity/LS2 separation with NEXT-BOUNDARY | closed | `plans/closure/mixed-router-interop/223-status.md` (`passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset`; type-0/256 + LS2 type-4/32, status 17 gone → `P223-NEXT-BOUNDARY [1,21]` on `0755dc1`) |
| M6 mixed-router interop | 224 NO_LEASESET lookup-path attribution | closed | `plans/closure/mixed-router-interop/224-status.md` (`passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap`; Router B answerable, helper client DB empty, exact trace unavailable on `895132c`) |
| M6 mixed-router interop | 225 NO_LEASESET lookup-path observability corrective | closed | `plans/closure/mixed-router-interop/225-status.md` (`passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution`; `P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B`) |
| M6 mixed-router interop | 226 Java loopback peer-diversity corrective | closed | `plans/closure/mixed-router-interop/226-status.md` (`passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary`; `P226-BASELINE-B-ZERO-HOP-UNKNOWN`, no topology correction admitted) |
| M6 mixed-router interop | 222 corrected client-NetDB/OCMOSJ narrowing | closed | `plans/closure/mixed-router-interop/222-status.md` (`passed-m6-java-client-netdb-ocmosj-narrowing-corrective`; exact preflight + nonce-tracked send → `P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION` on `cd334f8`) |
| M6 mixed-router interop | 220 Plan 219 diagnostic-attribution corrective | closed | `plans/closure/mixed-router-interop/220-status.md` (`passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required`; J219-B refuted; exact RI/PeerManager evidence retained; selector follow-up completed via Plan 222) |
| M6 mixed-router interop | 221 client-NetDB/OCMOSJ narrowing | superseded | `plans/closure/mixed-router-interop/221-status.md` (registered but not executed; superseded by Plan 222 after selector-equivalence/source review) |
| M6 mixed-router interop | 218 Java second-family final qualification | stopped | `plans/closure/mixed-router-interop/218-status.md` (reverse-delivery behavioral boundary retained; Plan 228 localized the build stop to missing paired tunnels; Plan 229 is the registered non-zero exploratory bootstrap corrective) |
| M6 mixed-router interop | 219 Java reverse-delivery root-cause investigation | superseded | `plans/closure/mixed-router-interop/219-status.md` (instrumentation retained; J219-B attribution superseded by Plan 220) |
| M6 mixed-router interop | 217 Java closure harness + evidence corrective | closed | `plans/closure/mixed-router-interop/217-status.md` |
| Service tunnels | 215 hosted Plan 214 re-verification | closed | `plans/closure/service-tunnels/215-status.md` |
| Service tunnels | 214 product HTTP/IRC closure | closed | `plans/closure/service-tunnels/214-status.md` |
| Service tunnels | 213 generic external qualification | closed | `plans/closure/service-tunnels/213-status.md` |
| Service tunnels | 212 router-backed material + inbound Streaming | closed | `plans/closure/service-tunnels/212-status.md` |
| M6 mixed-router interop | 200 publication observability + verified bootstrap | closed | `plans/closure/mixed-router-interop/200-status.md` |
| M6 mixed-router interop | 197 pq SSU2 option tolerance | closed | `plans/closure/mixed-router-interop/197-status.md` |
| M6 mixed-router interop | 196 controlled Java topology | closed | `plans/closure/mixed-router-interop/196-status.md` |
| I2CP | 172 independent LeaseSet2 lifecycle | closed | `plans/closure/i2cp/172-status.md` |
| I2CP | 171 invalid-preamble close corrective | closed | `plans/closure/i2cp/171-status.md` |
| SSU2 | 162 external-test lane isolation | closed | `plans/closure/ssu2/162-status.md` |
| SSU2 | 161 independent IPv4 interop | closed | `plans/closure/ssu2/161-status.md` |

Full per-plan history (000–232) lives in the subsystem roadmaps §7 tables.

## Superseded remote branches (do not merge)

| Branch | Base | Disposition |
|---|---|---|
| `origin/plan-m10-closure` | `ed81560` (Plan 198 lane) | Superseded. Holds the Plan 199 `registered-executable-m10-unified-final-closure` registration (plus a `TEMP` placeholder commit). Main governs: `plans/closure/service-tunnels/199-status.md` (`superseded-execution-decomposed-and-closed-via-plans200-204`) records the 199 attempt and its split into Plans 200–204, and M10 has since closed through Plans 210–215. Merging would revert the 199 status token and reintroduce flat-`plans/` links. Leave unmerged; delete only by explicit owner decision. |

## Execution order and dependency gates

- `m9_sequence = 164 -> 165 -> 166 -> 167 -> 168 -> 169 -> 171 -> 170 -> 172` (closed).
- `m10_sequence = 173 -> 174 -> 175 -> 176 -> 177 -> 178 -> 179 -> 180 -> 182 -> 181 -> 195 -> 202 -> 203 -> 206 -> 208 -> 210 -> 211 -> 212 -> 213 -> 214 -> 215 -> 204` (204 convergence open; 199/207/209 retained-superseded scaffolds).
- `m6_sequence = 183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218 -> 219 -> 220 -> 221 -> 222 -> 223 -> 224 -> 225 -> 226 -> 227 -> 228 -> 229 -> 230 -> 231 -> 232` (Plan 205 retained off-path; Plan 221 superseded-before-execution; Plan 222 closed the narrowing; Plan 223 closed identity separation with NEXT-BOUNDARY; Plan 224 closed with an observability gap; Plan 225 closed with exact lookup-path attribution; Plan 226 closed with exact baseline non-IP boundary; Plan 227 closed with selectable-C-but-not-built boundary; Plan 228 closed with NO-PAIRED-TUNNEL attribution; Plan 229 closed with NOT-EXPLORATORY-ELIGIBLE boundary; Plan 230 closed with the reverse-delivery boundary after proving predicate/correction/bootstrap/forward-delivery; Plan 231 closed with the exact target-IBGW-not-installed boundary after proving A-enqueue/C-OBEP with zero i2pr wire; Plan 232 owns the route-derived lease-gateway correction and Java second-family continuation).
- `active_plan = none`; `next_executable_plan = 232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure`; Plan 201 remains blocked pending Plan 232, Plan 204 remains blocked on M6 Java closure, and Plan 205 remains retained/deferred.

## Verification policy

Routine floor in `AGENTS.md` (fmt, check, test, clippy, doc, boundary scripts). Subsystem
evidence lanes are registered in each roadmap §9 and enforced by
`scripts/check-*-acceptance-evidence.sh` + `scripts/check-*-vectors.sh`. Environment-gated
lanes are `#[ignore]`-gated and fail-closed: missing env must fail, never silently pass.
Reference pins are frozen (i2pd `2.61.0`, Java I2P `2.13.0`, go-i2cp `b529ee1c…`).

## Deferred unregistered work

- Milestone 11 planning (pending M6 Java second-family closure).
- Non-loopback/remote I2CP, TLS/auth, broad historical I2CP compliance.
- Live/public NTCP2 or SSU2 router transport activation; broad mixed-router interop beyond
  the qualified lanes.
- Public I2P participation; network-transport-bound NetDB/public-router behavior.

Historical closure records MUST NOT be rewritten to conceal predecessor defects or failed
verification. Corrective passes own new closure evidence rather than editing history.
