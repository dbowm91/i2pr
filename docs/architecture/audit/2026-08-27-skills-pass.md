# Architecture Documentation Audit — 2026-08-27 (skills + top-level pass)

This audit covers the skill bundle surface (`.opencode/skills/`),
`AGENTS.md`, the top-level `docs/architecture.md`,
`docs/architecture/overview.md`, `README.md`, and the gaps left
behind by the 2026-08-27 parallel-subagent audit
(`docs/architecture/audit/2026-08-27-doc-audit.md`). It does **not**
re-audit the per-crate deep-dives in detail; the 2026-08-27 audit
already produced their corrected state.

## Scope of changes

### Skills (`.opencode/skills/`)

The previous surface shipped three skills (`i2pr-ntcp2-interop`,
`i2pr-rootless-sandbox`, `i2pr-multipass-recovery`) covering only
the closed NTCP2 interop lanes. Five skills now ship:

| Skill | Status | Notes |
| --- | --- | --- |
| `i2pr-local-dev` | **added** | Routine Milestone 6 + SAM baseline planning (Milestone 7) seam. Active development surface. |
| `i2pr-architecture` | **added** | Index for `docs/architecture/`, ADRs, plans, specs, and audit methodology. |
| `i2pr-ntcp2-interop` | replaced | Trimmed from 2043 lines to ~480 lines. Per-plan narratives removed; only the canonical closed-result state, the companion-skill pointers, the authoritative command surface, and the per-plan record index remain. |
| `i2pr-rootless-sandbox` | unchanged | Still authoritative for the Plan 046 rootless sealed-namespace lane. |
| `i2pr-multipass-recovery` | unchanged | Still authoritative for the Plan 048/049/050/051 Multipass recovery lane. |

Each new skill has a matching `agents/openai.yaml`.

The `i2pr-ntcp2-interop` rewrite removes the per-plan historical
narratives (Plans 042–100) and consolidates them into a single
status-table. The full per-plan authority table is retained at the
end for cross-reference. Future agents load this skill for harness
details, not as a history book — the closure records under `plans/`
remain the source of truth for historical state.

### `AGENTS.md`

- Added the `i2pr-architecture` skill reference to the **Read
  first** section (was: implicit through docs/architecture/).
- Replaced the implicit "Quick reference" crate list with an
  **Architecture index** block that links to `overview.md`,
  `dependency-graph.md`, `tooling.md`, `interop-apparatus.md`, and
  the per-crate deep-dives, and points at the `i2pr-architecture`
  skill as the navigation aid.
- Added an explicit **OpenCode skills** section with a 5-row table
  covering the full skill surface (was: implicit duplicates inside
  the **Interop and external evidence lanes** section).
- Removed the duplicate skill list inside the interop section; it
  pointed at the same three old skills and contradicted the new
  canonical skill list.
- Updated the **Read first** item 1 to name Plan 134 explicitly.
- Added a reference to `docs/architecture/audit/` in the **Read
  first** list.

### `docs/architecture.md` (top-level)

The 2026-08-27 audit did not cover the top-level narrative. This
pass found:

- The "four planes" table said "Client: Not implemented" — corrected
  to record Milestone 6 closure (Plan 134) and the next layer (SAM
  baseline planning, Milestone 7).
- The crate graph was stuck at the 9-crate state and did not include
  `i2pr-tunnel`, `i2pr-client`, `i2pr-netdb-persist`, or
  `tools/i2pr-interop`. Corrected to the 13-crate + 1-tool state.
- The "How data flows at runtime" section narrated every Milestone 5
  and Milestone 6 plan in long form (about 200 lines of historical
  content). Replaced with a structured step-by-step data flow that
  points at the closure records under `plans/`.
- The "Conventions" section was kept; the runtime/script-table now
  correctly enumerates the **nine** static boundary scripts (was:
  seven).
- Added a top-of-file pointer to
  `docs/architecture/overview.md` to keep this file short.

### `docs/architecture/overview.md`

The 2026-08-27 audit already applied surgical patches. This pass
applied further trimming:

- The crate index table is unchanged (already correct).
- The "How data flows at runtime" section still narrated every plan
  in detail (about 250 lines of historical content). Compressed to
  a structured step-by-step narrative that names each crate's role
  in the data flow and points at the closure records for the
  per-plan details.
- The **Authoritative reference lanes** section now names all five
  skills (was: three).

### `README.md`

- Added a top-of-file **Documentation map** paragraph pointing at
  `docs/architecture/`, the deep-dive index, the plans/ tree, and
  `.opencode/skills/`.
- Replaced the ~24-line block of per-plan narratives (Plans 119–134)
  with an 18-row Plan hierarchy table linking to each closure
  record. Each row carries the status token and the closure record
  path; per-plan detail lives in `plans/NNN-*.md` and
  `plans/NNN-status.md`.
- Trimmed the **MVP direction** section's per-plan narrative into a
  single paragraph naming Plan 134 as the current authority and SAM
  baseline planning as the next layer.
- Added a **Skill bundles** subsection to the **Architecture**
  section with one paragraph per skill.

### `docs/architecture/i2pr-runtime.md`

The 2026-08-27 audit flagged two missing modules (gap #2 in the
prior audit). This pass:

- Added `ntcp2_data_oracle` (`src/ntcp2_data_oracle.rs`, 13.9 K) and
  `ntcp2_handshake_observer` (`src/ntcp2_handshake_observer.rs`,
  5.3 K) to the **Module layout** table with their public types and
  responsibilities.
- Added the same two modules to the **Public surface** section,
  noting that both are compile-time-gated for the interop harness
  / testkit and never linked into the production daemon.

### `i2pr-rootless-sandbox` and `i2pr-multipass-recovery` (pre-existing doc/source drift)

The static boundary script `scripts/check-rootless-interop-boundary.sh`
references three Python files that were pruned by the Plan 099 harness
reduction on 2026-08-13 (`rootless_supervisor.py`,
`rootless_inner_runner.py`, `rootless_topology.py`,
`test_rootless_topology.py`). On the current host the script fails
with `rootless-owned file missing`. This is a pre-existing issue,
not caused by any change in this session. The script is only
invoked from the manual `workflow_dispatch` workflow
`.github/workflows/ntcp2-interop-rootless.yml`, so it does not fail
the main CI. This audit pass updated the
`i2pr-rootless-sandbox` and `i2pr-multipass-recovery` skills to
mark the historical file references as `Plan 046/053 historical —
pruned by the Plan 099 harness reduction on 2026-08-13` so an agent
loading the skill does not chase a missing file. Reconciling the
static boundary check's file list with the Plan 099 reduction is a
separate concern and is **not** in scope for this commit.

## Outstanding gaps not patched in this commit

These are recorded here for the next audit pass. They are not
blockers — every deep-dive is structurally accurate; the gaps are
content drift that future audits will catch as the source evolves.

1. **`i2pr-client.md`** — structurally accurate but stale in detail
   per the 2026-08-27 audit (gap #1). Specifically the source
   layout under `crates/i2pr-client/src/` is now `streaming/` with
   13 sub-files, not a flat `streaming.rs`. Missing:
   `Clock`/`SystemClock`/`ManualClock`, `StreamingTransport`,
   `RemoteDestination`/`RemoteDestinationKey`,
   `MAX_ROUTER_SIDE_LS2_BYTES_PER_REMOTE`, `LeaseSetSummary`,
   `DestinationShutdown`, `DestinationProgress`,
   `DestinationRouting::forget_remote`, and `plan132_trajectory.rs`.

2. **`i2pr-transport-ntcp2.md`** — `InitiatorPhase` has 9 variants
   (missing `AwaitCreatedPadding`); `ResponderPhase` has 7 variants
   (missing `AwaitRequestPadding`). Public methods
   `ResponderState::phase_label()`, `into_directional_states` are
   absent from the doc.

3. **`i2pr-daemon.md`** — line refs are drifting; doc references
   `main.rs:47-54` for `_command_name`; source is `main.rs:61`.

4. **`i2pr-testkit.md`** — `src/rng.rs` is 198 lines (doc says 182);
   `SchedulerError` is referenced as public but is not in the `pub
   use` re-exports.

5. **`interop-apparatus.md`** — substantially stale per the
   2026-08-27 audit (gap #5). ~30 files the doc describes as active
   no longer exist. Plan status tokens cite closure records that
   do not exist for Plans 093/094/100/101. The Plan 115–134 surface
   (Emissary, destination lifecycle, streaming, Milestone 6) is
   absent. The 2026-08-27 audit recommended a full rewrite when the
   next interop-surface plan lands. **Defer the rewrite to a
   separate commit.** The `i2pr-ntcp2-interop` skill now carries
   the canonical closed-state summary and the per-plan record index
   so the broken `interop-apparatus.md` is no longer load-bearing.

6. **AGENTS.md cross-references** — some skill descriptions point at
   specific closure records that may move over time. The skill
   descriptions intentionally link to stable plan numbers (Plan
   099/100 retained result, Plan 134 Milestone 6 authority). When a
   new Milestone authority lands, update both `AGENTS.md` and the
   affected skills in the same commit.

## Methodology

1. Read the 2026-08-27 audit document to find the load-bearing gaps.
2. Survey `.opencode/skills/` against the active plan surface
   (Milestone 6 closed via Plan 134; SAM baseline planning next).
3. Drafted `i2pr-local-dev` and `i2pr-architecture` skill bundles
   with the standard outline (description + front matter + sections
   matching the i2pr-ntcp2-interop structure).
4. Rewrote `i2pr-ntcp2-interop` to remove per-plan historical
   narratives and consolidate them into a single status table.
5. Updated `AGENTS.md` to add the skill table and the architecture
   index block.
6. Replaced the top-level `docs/architecture.md` "How data flows at
   runtime" section and crate graph to reflect the current
   13-crate + 1-tool workspace.
7. Trimmed `docs/architecture/overview.md` data-flow narrative.
8. Replaced the README per-plan narrative block with a status
   table.
9. Added the two missing `i2pr-runtime` modules (`ntcp2_data_oracle`
   and `ntcp2_handshake_observer`) to `i2pr-runtime.md`.

The static boundary scripts and the focus-seam test commands were
not touched; they were already accurate. The pre-handoff sequence
in `AGENTS.md` was already accurate and is now reinforced by the
new skill table.
