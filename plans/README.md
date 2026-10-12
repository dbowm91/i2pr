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
- A **new architectural scope, owner, durable dependency, or corrective successor to a
  finalized blocked closure** requires a new plan referencing the authoritative closure.
  **An unresolved test fixture or source defect discovered during an active implementation
  pass is fixed and rerun in that same plan**, with regression and sanitized evidence; it
  does not trigger a new numbered plan merely because a bounded attempt failed.
- Every subsystem roadmap and implementation plan MUST distinguish **invariant** (must always
  remain true), **capability** (user/operator-visible behavior), **infrastructure** (internal
  machinery — MUST NOT be presented as completed capability without a consumer), and
  **polish** (ergonomics/diagnostics/cleanup/docs).
- A milestone is dependency-ready only when every hard dependency is closed and every
  interface dependency has a stable written contract.

## Bounded execution and causal corrective policy

This is a **planning/execution rule**, not a relaxation of `GUARDRAILS.md`,
`specs/CONFORMANCE.md`, `specs/support.toml` or any executed closure.

1. **Identify the smallest independent proof.** Maintain distinct signed-RI admission,
   peer eligibility, source-owned message trigger, TCP/Noise authentication, I2NP receipt,
   router product effect, legitimate correlated response, fresh reverse initiation,
   tunnel build and external SAM/Streaming acceptance. Earlier lower-tier successes
   remain valid evidence; later failure cannot erase them. Loopback is valid for local
   transport but is not proof of independent network/public reachability.
2. **Preflight source-owned prerequisites before process launch.** Inspect pinned reference
   call graph and selection filters; verify exact identity, RI signature/version/netId,
   address/family/caps, local bind, reference datadir, authorized address scope,
   NetDB membership/bootstrap size and chosen operation. Test stock-to-stock *when a
   stock topology or selection failure is ambiguous*, not as an unconditional gate before
   an independent successful mixed-router operation.
3. **One workstream, causal iterations.** Record the first missing event and its owning
   source/fixture stage. An unchanged topology and source cannot be retried indefinitely.
   An **identified cause** (including reference fixture/env mistakes, stdout parsing or
   product wiring) permits a correction, pre/post deterministic regression and rerun
   **inside the still-active plan**, respecting bounded children/time/memory/traffic.
   No rule fixes a global attempt count of one; budget reflects realistic bootstrapping
   and permits progress after a real change. Only new architectural scope, different
   owner or a finalized blocked closure requiring a successor merits a new plan number.
4. **Closure only at a stable disposition.** `passed` requires executed plan acceptance;
   `blocked` requires source-attributed missing prerequisite, failed source-level fix,
   genuinely unavailable authorized environment, or exhausted bounded *causal* options.
   Capture stage, topology, exact command/commit/reference pin, expected and observed
   effect, source owner, corrective delta and next executable step. Do not create a
   terminal `blocked` closure just because one first-run fixture/observer was wrong.
   Never rewrite a historical closure: record a new status or explicit successor per
   existing authority rules; registry projections cannot promote protocol claims.
5. **Privacy and conformance remain hard.** Exact pinned unmodified references, no
   root/namespaces/VM/public-I2P routine traffic, no helper directly invoking a private
   stock transport API, no forged reachability/bandwidth/capabilities, and no log of
   raw RouterInfos, destinations, secrets or full identities. Deliberately selected
   environment-gated lanes fail hard when tooling is absent; unsupported support
   inventory stays unadvertised.
6. **Handoff stop conditions are actionable.** A failed selector means inspect its
   admitted set/exclusion reasons before attempting Noise; a verified decoded message
   without product consumption goes to the product owner; a tunnel-build timeout
   cannot block a separate valid NTCP2 request/reply proof. Escalate to a new plan
   only when the current owner cannot repair the established root cause within scope.

For practical NTCP2 applications of this policy, use the exact-pinned source findings
and successor plans in `subsystems/ntcp2-transport-roadmap.md`.

## Naming conventions (i2pr divergence from codegg — intentional)

Codegg numbers milestones locally per subsystem. i2pr preserves its **global `NNN` plan
numbers** in every filename because status tokens (`plan_134`), supersession chains
(`superseded-by-plan172`), checker scripts, and 100+ doc/spec cross-references are keyed on
them. Renumbering would destroy traceability.

- Subsystem roadmap: `subsystems/<subsystem>-roadmap.md` (stable kebab-case, no dates).
- Implementation plan: `implementation/<subsystem>/NNN-short-title.md` (global NNN kept).
- Closure record: `closure/<subsystem>/NNN-status.md` (same global NNN as the plan).
- New implementation-plan numbers have one owning subsystem. The exact historical
  Proposal 170/296, Proposal 170/297, Anonymity/296, Anonymity/297, and the qualified
  Plan 349 (three owners), 350, 351, and 352 collisions are recorded in
  [`global-number-collision-ledger.md`](global-number-collision-ledger.md) and checked
  by `scripts/check-global-plan-number-uniqueness.py`. These finite path exceptions do
  not permit any additional owner for those numbers.
- **ADR numbers are load-bearing in the same way.** The ledger also records the ADR
  0030, 0032, and 0033 collisions across the managed-app, portable-service-tunnel, and
  Proposal 170 lines. Prose discussing a colliding ADR must qualify it
  (for example `ADR 0032 (Proposal 170)`). Since Plan 361, ADR-number uniqueness is
  enforced by `scripts/check-adr-number-uniqueness.py` alongside the plan-number check;
  it tolerates exactly those three recorded collisions as explicit filenames and fails
  closed on a duplicate, on an unparseable ADR filename, and on an exemption whose file
  no longer exists. Filing a new ADR still needs a free number, but the floor now
  catches a collision.
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
