# i2pr Planning System

This directory separates durable architectural direction from interim execution planning,
following the codegg `planning/registry` format adapted to i2pr's history.

## Canonical direction (stable — reference, do not duplicate)

- `GUARDRAILS.md` — non-negotiable security/architecture constraints.
- `specs/CONFORMANCE.md` — what counts as protocol-support evidence.
- `specs/support.toml` — machine-readable support inventory.
- `docs/adr/` — architecture decision records (accepted ADRs are superseded, not rewritten).

Interim plans MUST reference these rather than copying or silently revising them.

## Planning hierarchy

```text
GUARDRAILS + specs/CONFORMANCE + specs/support.toml  (canonical, stable)
        |
        v
Architecture decision records (docs/adr/)             (durable decisions)
        |
        v
Subsystem roadmaps (plans/subsystems/)                (coherent workstreams)
        |
        v
Milestone implementation plans                        (bounded agent handoff)
        |
        v
Implementation and verification
        |
        v
Closure records (plans/closure/)                      (gate that determines completion)
        |
        v
Archive (plans/archive/)                              (historical traceability)
```

## Directory roles

- `subsystems/` — one roadmap per workstream (`<subsystem>-roadmap.md`), each with a
  milestone table linking implementation plans to closure records.
- `implementation/<subsystem>/` — bounded plan narratives handed to implementation agents.
- `closure/<subsystem>/` — status records (authoritative) plus supporting completion narratives.
- `archive/` — frozen snapshots, including the byte-identical pre-migration flat registry.
- `diagnostics/` — point-in-time diagnostic reports (kept as-is; not planning authority).
- `registry.md` — compact control surface: active roadmaps, dependency-ready plans,
  blockers, recent closures. Links to source documents; never duplicates them.

## Authority order (i2pr — wins over prose, always)

```text
plans/closure/<subsystem>/*-status.md > executable tests/scripts > ADRs > prose
```

Closure/status records (`*-status.md`) are authoritative. Per-plan narratives are historical
context unless the newest status points to them as executable work. Never mark a row `passed`
because prose says so; it must derive from an executed command. Newest explicit
`superseded-by-*` status wins on conflict.

## Core rules

- Long-term direction states what i2pr is becoming and what must remain true. Interim
  documents state what an agent should implement next against a specific repository baseline.
- No milestone is complete merely because code landed. Completion requires the closure
  evidence defined by its plan and subsystem roadmap.
- One milestone = one coherent pass for one implementation agent.
- Corrective passes are new plans referencing the original, not amendments pretending the
  original succeeded.
- Every subsystem roadmap and implementation plan MUST distinguish **invariant** (must always
  remain true), **capability** (user/operator-visible behavior), **infrastructure** (internal
  machinery — MUST NOT be presented as completed capability without a consumer), and
  **polish** (ergonomics/diagnostics/cleanup/docs).
- A milestone is dependency-ready only when every hard dependency is closed and every
  interface dependency has a stable written contract.

## Naming conventions (i2pr divergence from codegg — intentional)

Codegg numbers milestones locally per subsystem. i2pr preserves its **global `NNN` plan
numbers** in every filename because status tokens (`plan_134`), supersession chains
(`superseded-by-plan172`), checker scripts, and 100+ doc/spec cross-references are keyed on
them. Renumbering would destroy traceability.

- Subsystem roadmap: `subsystems/<subsystem>-roadmap.md` (stable kebab-case, no dates).
- Implementation plan: `implementation/<subsystem>/NNN-short-title.md` (global NNN kept).
- Closure record: `closure/<subsystem>/NNN-status.md` (same global NNN as the plan).
- Early-era records kept as-is: `closure/<subsystem>/NNN-closure.md`,
  `NNN-closure-attempt.md`, `NNN-candidate.md`, status-amending `*-amendment-*.md` files.
- Non-standard but authoritative: `closure/mixed-router-interop/193-streaming-status.md`.
- Archived document: `archive/legacy-flat-registry-2026-09-18.md` (byte-identical snapshot
  of the pre-migration `plans/README.md`; internal relative links resolve against the old
  flat layout — use `registry.md` for live authority).

## Status vocabulary

i2pr status tokens are free-form (`passed-*`, `retained-*`, `superseded-by-*`, `blocked-*`,
`stopped-*`, `in-progress-*`, `registered-*`). `registry.md` projects each onto the codegg
state vocabulary (`closed`, `retained`, `superseded`, `blocked`, `active`, `ready`,
`archived`); the i2pr token itself stays authoritative.

## Planning lifecycle

1. Identify the relevant canonical sections and invariants (`GUARDRAILS.md`, `specs/`).
2. Record unresolved architectural decisions in `docs/adr/`.
3. Create or update the subsystem roadmap in `subsystems/`.
4. Select one dependency-ready milestone; write the bounded handoff plan under
   `implementation/<subsystem>/`.
5. Register it in `registry.md` (`ready` → `active` when work begins).
6. Implement and verify (routine floor in `AGENTS.md`; subsystem lanes in the roadmap).
7. Write the closure record under `closure/<subsystem>/`.
8. Update `registry.md` and the subsystem roadmap status; audit blocked work and unblock
   dependency-ready plans in the same commit.
9. Archive superseded interim documents when they no longer represent active work,
   preserving traceability via `git mv`.

## Starting a new workstream

Begin with `subsystems/` (copy the 12-section structure from the newest roadmap), then
`implementation/`, `closure/`, and `archive/` `README.md` files for the per-class templates.
Register active work in `registry.md` before handing implementation plans to agents.

## Pre-migration history

Plans 000–215 lived flat as `plans/NNN-*.md` with a 76 KB `plans/README.md` authority
registry. That registry is frozen at `archive/legacy-flat-registry-2026-09-18.md`; its live
classifications were projected into `registry.md` and the subsystem roadmaps. Git history
preserves every file via rename detection (`git log --follow`).
