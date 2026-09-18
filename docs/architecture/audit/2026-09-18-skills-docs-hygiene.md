# Skills + docs hygiene pass — 2026-09-18

Follow-up to `2026-09-18-doc-audit.md` (per-crate deep-dives) and
`2026-08-27-skills-pass.md`. This pass covers the skill bundles, the
plan-authority mirrors (`README.md`, `plans/README.md`), and the
top-level architecture docs. No production code changed.

## Skills (`.opencode/skills/` canonical; `.agents/skills` is a symlink)

| Skill | Fix |
| --- | --- |
| `i2pr-local-dev` | Frontmatter `description` trimmed 8454 → ~330 chars (was a full plan history; broke skill routing). Authority block synced to status files: plan_181/195/201/204 tokens, added Plan 205/208/210–215 rows, refreshed `next_executable_plan`. M10 read-order now leads with Plans 215/214/213/212 instead of the blocked Plan 181 entry; the Plan 203-driver flip paragraph now cites the Plan 214 driver + Plan 215 §18 transition. Routine floor synced with `AGENTS.md` (added service-tunnel-boundaries, exploratory/netdb/destination/streaming/m6-mixed evidence checkers, `--doc` tests; unittest scope narrowed to `test_execution_lane.py`). Final-claim rules: Plan 188 `in-progress` → historical, Plan 181 → status token, handoff tail cites 212–215 + 205. |
| `i2pr-architecture` | Plan index: Plan 188 `in-progress` → historical; Plan 181 → status token with 214/215 closure; Plan 194 → status token with 198/200/201 re-scope note; Plan 196 `failed → passed` flip recorded; Plan 201 → Branch C/D token with Plan 205 next; added Plan 205/208/210–215 entries; Plan 195 → status token. Scripts table: added the four tunnel-evidence rows, refreshed the service-tunnel-acceptance and m6-mixed rows. Removed a leftover `Plan 194 retains ...` line that contradicted the new 194 paragraph. |
| `i2pr-ntcp2-interop` | Added HISTORICAL banner (closed Plans 099/100 lane; not for routine work per `AGENTS.md`; frozen NTCP2-era pins ≠ current 2.13.0/2.61.0 pins). `Plan 102 active authority` → historical note (M4 closed; current authority in `plans/README.md`). |
| `i2pr-rootless-sandbox` | Added HISTORICAL banner (Plan 046 belongs to the closed NTCP2 sequence). |
| `i2pr-multipass-recovery` | Added HISTORICAL banner (Plans 048–051 belong to the closed NTCP2 sequence). |

## Authority mirrors

`README.md` classification block: plan_194/201/204/208/209/195/181 tokens
synced verbatim to `plans/*-status.md`; removed the duplicated
plan_203/207/208/209 + stale `milestone10_remote_service_interop` second copy.
`plans/README.md` classification block: plan_171 (`-retained` suffix dropped),
plan_181, plan_194, plan_200 (truncation), plan_201, plan_214 synced; added
missing plan_204/plan_215 rows; `m10_remote_application_interop` and
`next_m10_application_plan` updated to the Plan 215 double-pass outcome.

## Top-level docs

| Doc | Fix |
| --- | --- |
| `docs/architecture.md` | Client/Service plane rows (SAM "next" → closed via 151; service tunnels "Not implemented" → M10 174–180/182 + 213–215); crate count 13 → 16 (15 production + testkit); script table gained the 13 missing post-M3 checkers and notes `check-plan095-workflow.sh` as pruned; SAM paragraph now cites Plan 151/172/214–215 closure; deep-dive count 13 → 16. |
| `docs/architecture/tooling.md` | Members 13 → 16 crates (+`i2pr-transport-ssu2`, `i2pr-api`, `i2pr-service-tunnels`); `check-plan095-workflow.sh` row marked pruned; NTCP2 synthetic-lane pins labelled frozen NTCP2-era (current pins per `AGENTS.md`). |
| `docs/architecture/dependency-graph.md` | `i2pr-api` allowlist gains `i2pr-tunnel` (matches `check-dependency-direction.sh` + `crates/i2pr-api/Cargo.toml`); reverse rule corrected. (The 2026-09-18 deep-dive audit had marked this file GOOD; the edge drift was re-verified against the script before patching — doc follows the enforced script.) |
| `docs/protocol-support.md` | Service-tunnels row "Not implemented" → experimental loopback-only with Plans 214–215 evidence; "through the current Milestone 3 corrective integration" → Milestone 10 closure. |
| `AGENTS.md` | Added `Skills and architecture index` section (canonical skill location + `.agents/skills` symlink note + historical-skill rule + architecture entry points); trimmed the duplicated skill clause from the workspace-boundaries paragraph. |

## Deliberately not changed

- `plans/*-status.md` (authoritative; never edited in a hygiene pass).
- Plan 196 `passed-...` rows in the mirrors: `plans/196-status.md:3` still reads
  `in-progress-...-pending-external-re-run`, but downstream Plans 197/200/201
  treat the corrective as landed. Per newest-status-wins, the mirrors keep
  `passed`; resolving the 196 status token itself needs a plan owner, not a
  hygiene pass.
- Per-crate deep-dive plan-history narratives beyond the 2026-09-18 audit scope.
- `docs/architecture/interop-apparatus.md` (rewrite already deferred), the
  `protocol-support.md` Plan 095/096 historical prose (correctly describes the
  pruned script in past tense), `CONTRIBUTING.md`/`GUARDRAILS.md` (no drift found
  beyond the already-documented stale `CONTRIBUTING.md` cargo snippets).
- No `.skills/` directory exists and none was created; agents looking for it
  should use `.opencode/skills/` (or the `.agents/skills` symlink).
