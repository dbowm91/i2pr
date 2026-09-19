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
| Service tunnels | active | `plans/subsystems/service-tunnels-roadmap.md` | Plan 215 passed; Plan 204 convergence open | Blocked on independent M6 Java second-family closure; Plan 224 is the active NO_LEASESET lookup-path attribution |
| M6 mixed-router interop | active | `plans/subsystems/mixed-router-interop-roadmap.md` | Plan 224 ready: NO_LEASESET lookup-path attribution | Plan 223 moved the reverse send to ACCEPTED→NO_LEASESET (21). Plan 224 first proves Router B main-NetDB answerability, then traces the exact A→B query, B answer, A client-tunnel reply, and helper client-subDB store; no corrective is authorized until attribution closes |

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
- **M6 mixed-router program**: Plan 217 closed the harness corrective; Plan 218 retains the reverse-delivery behavioral stop; Plan 219 attribution is superseded; Plan 220 refuted J219-B; Plan 221 is superseded-before-execution; Plan 222 narrowed OCMOSJ to status 17; Plan 223 corrected Destination/LS2 separation and moved the tracked send to `ACCEPTED -> NO_LEASESET (21)`. Plan 224 is now the sole dependency-ready attribution pass and must identify the earliest missing lookup stage before any Plan-225 corrective.

## Dependency-ready and active plans

| Subsystem | Plan | State | Handoff | Dependencies / handoff note |
|---|---|---|---|---|
| M6 mixed-router interop | 224 NO_LEASESET lookup-path attribution | ready | `plans/implementation/mixed-router-interop/224-m6-java-no-leaseset-lookup-path-attribution.md` | Attribution-only. First prove Router B main-NetDB current/RAP LS state; only if answerable, trace exact helper client lookup A→B, B answer, A inbound-client DSM, and client-subDB installation. The resulting fix belongs to Plan 225. |

## Blocked work

| Subsystem | Plan | Blocker |
|---|---|---|
| M6 mixed-router interop | 201 | Blocked pending Plan 224 NO_LEASESET lookup-path attribution and its evidence-supported successor; no J219-B bootstrap, SAM, or topology corrective is authorized |
| Service tunnels | 204 | Cross-milestone convergence waits for M6 Java second-family closure; Plan 224 is attribution-only and does not alter M10 product authority |
| M6 mixed-router interop | 187 / 188 / 191 (historical) | Retained `blocked`/`stopped` tokens; rows partially flipped by Plans 190/192/193 — see roadmap |

### Retained / conditional work

- **Plan 205 SAM/helper pivot** — `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`. J219-B remains refuted; Plan 223 targeted the Java target-Destination/LS2 crypto boundary below the SAM API and closed with NO_LEASESET (also below SAM), so no pivot reconsideration is authorized.

## Recently closed work

| Subsystem | Plan | State | Closure record |
|---|---|---|---|
| M6 mixed-router interop | 223 identity/LS2 separation with NEXT-BOUNDARY | closed | `plans/closure/mixed-router-interop/223-status.md` (`passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset`; type-0/256 + LS2 type-4/32, status 17 gone → `P223-NEXT-BOUNDARY [1,21]` on `0755dc1`) |
| M6 mixed-router interop | 222 corrected client-NetDB/OCMOSJ narrowing | closed | `plans/closure/mixed-router-interop/222-status.md` (`passed-m6-java-client-netdb-ocmosj-narrowing-corrective`; exact preflight + nonce-tracked send → `P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION` on `cd334f8`) |
| M6 mixed-router interop | 220 Plan 219 diagnostic-attribution corrective | closed | `plans/closure/mixed-router-interop/220-status.md` (`passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required`; J219-B refuted; exact RI/PeerManager evidence retained; selector follow-up completed via Plan 222) |
| M6 mixed-router interop | 221 client-NetDB/OCMOSJ narrowing | superseded | `plans/closure/mixed-router-interop/221-status.md` (registered but not executed; superseded by Plan 222 after selector-equivalence/source review) |
| M6 mixed-router interop | 218 Java second-family final qualification | stopped | `plans/closure/mixed-router-interop/218-status.md` (reverse-delivery behavioral boundary retained; Plan 222 narrowed it to status 17; Plan 223 owns the registered corrective) |
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

Full per-plan history (000–224) lives in the subsystem roadmaps §7 tables.

## Superseded remote branches (do not merge)

| Branch | Base | Disposition |
|---|---|---|
| `origin/plan-m10-closure` | `ed81560` (Plan 198 lane) | Superseded. Holds the Plan 199 `registered-executable-m10-unified-final-closure` registration (plus a `TEMP` placeholder commit). Main governs: `plans/closure/service-tunnels/199-status.md` (`superseded-execution-decomposed-and-closed-via-plans200-204`) records the 199 attempt and its split into Plans 200–204, and M10 has since closed through Plans 210–215. Merging would revert the 199 status token and reintroduce flat-`plans/` links. Leave unmerged; delete only by explicit owner decision. |

## Execution order and dependency gates

- `m9_sequence = 164 -> 165 -> 166 -> 167 -> 168 -> 169 -> 171 -> 170 -> 172` (closed).
- `m10_sequence = 173 -> 174 -> 175 -> 176 -> 177 -> 178 -> 179 -> 180 -> 182 -> 181 -> 195 -> 202 -> 203 -> 206 -> 208 -> 210 -> 211 -> 212 -> 213 -> 214 -> 215 -> 204` (204 convergence open; 199/207/209 retained-superseded scaffolds).
- `m6_sequence = 183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218 -> 219 -> 220 -> 221 -> 222 -> 223 -> 224` (Plan 205 retained off-path; Plan 221 superseded-before-execution; Plan 222 closed the narrowing; Plan 223 closed the identity separation with NEXT-BOUNDARY; Plan 224 to be registered for NO_LEASESET).
- `next_executable_plan = 224-m6-java-no-leaseset-lookup-path-attribution`; Plan 224 is attribution-only and no production corrective is authorized until it closes.

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
