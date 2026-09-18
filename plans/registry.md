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
| Service tunnels | active | `plans/subsystems/service-tunnels-roadmap.md` | Plan 215 passed; Plan 204 convergence open | Blocked only on independent M6 Java second-family closure via Plan 218 or an explicitly registered fallback |
| M6 mixed-router interop | active | `plans/subsystems/mixed-router-interop-roadmap.md` | Plan 217 closed; Plan 218 ready | Plan 217 closed the harness/evidence corrective; Plan 218 owns the fresh exact-head external classification on the corrected harness; Plan 205 retained as conditional fallback only |

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
- **M6 mixed-router program**: Plan 217 closed the Java closure harness and evidence corrective; Plan 218 is the executable head and owns final Java-family qualification. Plan 201 stays blocked until Plan 218 records the fresh terminal classification on the corrected harness — `plans/closure/mixed-router-interop/201-status.md`, `plans/closure/mixed-router-interop/217-status.md`, `plans/closure/mixed-router-interop/218-status.md`.

## Dependency-ready and active plans

| Subsystem | Plan | State | Handoff | Dependencies / handoff note |
|---|---|---|---|---|
| M6 mixed-router interop | 218 Java second-family final qualification | ready | `plans/implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md` | Run the corrected `run-java.sh` harness end-to-end against the exact-pinned Java I2P 2.13.0 cache; record the fresh terminal `P200-*` classification; flip the seven §11 stop rows `blocked → passed` on the corrected harness |
| Service tunnels | 204 convergence | blocked | `plans/closure/service-tunnels/204-status.md` | M10 product closure is already retained; final cross-milestone normalization waits for M6 Java second-family closure via Plan 218 |

## Blocked work

| Subsystem | Plan | Blocker |
|---|---|---|
| M6 mixed-router interop | 201 | Hard-blocked on Plan 218 fresh exact-head external run consuming the corrected-harness terminal `P200-*` classification (Plan 217 closure unblocked the harness, not the classification itself) |
| Service tunnels | 204 | Cross-milestone convergence waits for M6 Java second-family closure (Plan 218 success or a later explicitly registered fallback) |
| M6 mixed-router interop | 187 / 188 / 191 (historical) | Retained `blocked`/`stopped` tokens; rows partially flipped by Plans 190/192/193 — see roadmap |

### Retained / conditional work

- **Plan 205 SAM/helper pivot** — `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`. Reactivate only if Plan 218 proves a genuine stock-Java direct-I2CP public-client boundary after the corrected Plan 217 harness is in place.

## Recently closed work

| Subsystem | Plan | State | Closure record |
|---|---|---|---|
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

Full per-plan history (000–218) lives in the subsystem roadmaps §7 tables.

## Superseded remote branches (do not merge)

| Branch | Base | Disposition |
|---|---|---|
| `origin/plan-m10-closure` | `ed81560` (Plan 198 lane) | Superseded. Holds the Plan 199 `registered-executable-m10-unified-final-closure` registration (plus a `TEMP` placeholder commit). Main governs: `plans/closure/service-tunnels/199-status.md` (`superseded-execution-decomposed-and-closed-via-plans200-204`) records the 199 attempt and its split into Plans 200–204, and M10 has since closed through Plans 210–215. Merging would revert the 199 status token and reintroduce flat-`plans/` links. Leave unmerged; delete only by explicit owner decision. |

## Execution order and dependency gates

- `m9_sequence = 164 -> 165 -> 166 -> 167 -> 168 -> 169 -> 171 -> 170 -> 172` (closed).
- `m10_sequence = 173 -> 174 -> 175 -> 176 -> 177 -> 178 -> 179 -> 180 -> 182 -> 181 -> 195 -> 202 -> 203 -> 206 -> 208 -> 210 -> 211 -> 212 -> 213 -> 214 -> 204` (204 convergence open; 199/207/209 retained-superseded scaffolds).
- `m6_sequence = 183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218` (Plan 205 retained as a conditional fallback branch, not on the critical path).
- `next_executable_plan = 218-m6-java-second-family-final-qualification`; Plan 201 re-evaluates after Plan 218 records its terminal classification; Plan 204 convergence follows successful Java-family closure.

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
