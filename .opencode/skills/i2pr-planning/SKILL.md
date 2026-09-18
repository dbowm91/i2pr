---
name: i2pr-planning
description: i2pr plans/ planning process — registering implementation plans, writing closure records, maintaining registry.md and subsystem roadmaps, and closing milestones. Use when an agent is asked to register a new plan, close out an implementation plan, update the planning registry, create a subsystem roadmap, run the unblock audit, or archive superseded planning.
---

# I2PR Planning Process

How interim planning under `plans/` works: subsystem roadmaps, milestone implementation
plans, closure records, `registry.md`, and archive. Canonical direction lives outside
`plans/` (`GUARDRAILS.md`, `specs/CONFORMANCE.md`, `specs/support.toml`,
`specs/README.md` + `specs/protocols/` dossiers, `docs/adr/`);
interim plans MUST reference it, never silently revise it.

Load this skill when working on:

- Registering a new implementation plan under `plans/implementation/<subsystem>/`
- Closing out an implementation plan with a record under `plans/closure/<subsystem>/`
- Updating `plans/registry.md` (ready/active/blocked/closed transitions, unblock audit)
- Creating or revising a subsystem roadmap under `plans/subsystems/`
- Archiving superseded planning under `plans/archive/`
- Understanding status tokens (`passed-*`, `retained-*`, `superseded-by-*`, `blocked-*`,
  `in-progress-*`, `registered-*`) and their codegg-state projection

For crate/ADR/spec navigation and doc-vs-source audits, see `i2pr-architecture`. For
product/SSU2/SAM/I2CP/tunnel implementation rules, see `i2pr-local-dev`.

## Authority order (non-negotiable)

```text
plans/closure/<subsystem>/*-status.md > executable tests/scripts > ADRs > prose
```

Closure/status records are authoritative; narratives are historical context unless the
newest status points at them as executable work. Never mark a row `passed` because prose
says so — it must derive from an executed command. Newest explicit `superseded-by-*`
status wins on conflict. Historical records MUST NOT be rewritten to conceal predecessor
defects or failed verification.

## Layout and naming (global NNN is load-bearing — never renumber)

- Subsystem roadmap: `plans/subsystems/<subsystem>-roadmap.md` (stable kebab-case)
- Implementation plan: `plans/implementation/<subsystem>/NNN-short-title.md`
- Closure record: `plans/closure/<subsystem>/NNN-status.md` (same NNN as the plan)
- Early-era records kept as-is: `NNN-closure.md`, `NNN-candidate.md`, `*-amendment-*.md`
- Registry: `plans/registry.md` (compact control surface; links, never duplicates)
- Archive: `plans/archive/` (frozen snapshots via `git mv`)
- `plans/diagnostics/` holds point-in-time reports, not planning authority

Status tokens (`plan_134`), supersession chains (`superseded-by-plan172`), evidence
checkers, and 100+ doc/spec cross-references are keyed on global plan numbers.
Renumbering destroys traceability. See `plans/README.md` for the full guide and the
pre-migration history.

## Status vocabulary

i2pr tokens are free-form; `plans/registry.md` projects them onto codegg states:

| i2pr token shape | State | Meaning |
|---|---|---|
| `passed-*` | `closed` | Closure record accepted; evidence landed |
| `retained-*` (no `superseded-by`) | `retained` | Evidence kept; interpretation narrowed |
| `*-superseded-by-planNNN` | `superseded` | Replaced; the superseding plan governs |
| `blocked-*`, `stopped-*` | `blocked` | Named dependency/evidence gap blocks progress |
| `in-progress-*` | `active` | Implementation or closure work in progress |
| `registered-*`, `pending`, `next *` | `ready` | Approved for execution once dependencies clear |
| `skipped`, `retired*` | `archived` | Never authoritative; traceability only |

## Lifecycle

1. Identify canonical sections and invariants (`GUARDRAILS.md`, `specs/`).
2. Record unresolved architectural decisions in `docs/adr/` (new ADR; accepted ADRs are
   superseded, never rewritten).
3. Create or update the subsystem roadmap (12-section structure in
   `plans/subsystems/README.md`), with the milestone row in its §7 table.
4. Write the bounded handoff plan under `plans/implementation/<subsystem>/`
   (template in `plans/implementation/README.md`): one outcome, why-ready, evidence,
   invariants, in/out scope, production changes, work packages, failure/cancel/restart
   semantics, compat/migration, tests, exact verification commands, docs, acceptance and
   stop conditions, closure evidence required, handoff notes.
5. Register it in `plans/registry.md` as `ready` (→ `active` when work begins).
6. Implement and verify (routine floor in `AGENTS.md`; subsystem lane in the roadmap §9).
7. Write the closure record under `plans/closure/<subsystem>/` (requirements in
   `plans/closure/README.md`): commits, requirement-to-evidence matrix, commands run with
   outcomes (label local vs CI truthfully), invariant/failure/migration/security reviews,
   docs/ops, limitations, findings by severity, roadmap disposition.
8. Update `plans/registry.md` and the roadmap §7 row; run the unblock audit below.
9. Archive superseded interim documents via `git mv`, preserving traceability.

## Registering an implementation plan

- Confirm the repository baseline is current and every hard dependency is closed
  (interface deps need a stable written contract).
- Confirm unresolved decisions have ADRs or are explicitly out of scope.
- Size for one coherent pass: one ownership boundary, production changes, focused tests,
  verification, docs. Prefer a vertical slice with a consumer over a horizontal refactor.
- Classify the work: **invariant** (static guards/property tests), **capability**
  (end-to-end acceptance), **infrastructure** (contracts + tests; MUST NOT be presented
  as completed capability), **polish** (after correctness closure).
- Add the `registry.md` row (subsystem, plan, state, handoff path, dependencies) in the
  same commit. One commit = one status change.

## Closing out an implementation plan

A milestone MUST NOT be marked closed when only compilation/formatting was verified,
required tests were not run, a user-visible capability has only internal infrastructure,
a security/migration requirement is unimplemented, or a known high-severity defect
remains. It MAY be `retained-*`/`conditionally closed` only with the named condition,
risk, and exact future evidence stated.

Evidence rules (fail-closed, enforced by `scripts/check-*-acceptance-evidence.sh`):

- Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs
  require `--ignored --exact`, and missing env must **fail**, never silently pass.
- Forbidden: `|| true`, `continue-on-error`, filename filtering, fake peer env, broad
  exclusions, early-return-success, production wire changes to go green, patching or
  vendoring external routers/clients.
- Reference pins are frozen (i2pd `2.61.0`, Java I2P `2.13.0`, go-i2cp `b529ee1c…`);
  do not change them without a new plan-of-record. Raw reference logs are never
  evidence — only sanitized counts/hashes reach evidence files.
- New deps need review (purpose, transitive impact, `unsafe` exposure, features,
  license); workspace versions stay centralized with narrow default features.
- Never claim capability/version/RouterInfo/SAM/I2CP behavior beyond the tested subset
  in `specs/CONFORMANCE.md`.

## Unblock audit (required at every closure)

When writing a closure record, audit `registry.md` blocked work plus affected roadmap
dependency graphs: for each registered plan listing the just-closed milestone as a hard
or interface dependency, check whether ALL other hard deps are closed and interface deps
have stable contracts. If yes, move it to `ready` in the same commit and record the
audit in the closure record. If a corrective pass is required, register it under the
same subsystem immediately. Never silently unblock.

## Corrective passes

A corrective pass is a NEW plan in the same subsystem referencing the original plan and
closure record, listing each unclosed requirement or discovered defect, explaining why
original verification missed it, and adding regression tests or guards. Repeated
correctives mean the roadmap or milestone sizing must be revised.

## Registry maintenance

`plans/registry.md` links active documents and blockers without duplicating them:
active roadmaps, dependency-ready plans, active/closing work, blocked work with owners,
recently closed work with commits, execution sequences, verification policy, deferred
work. Remove closed rows from active sections after recording them under recently
closed. Do not copy milestone requirements into the registry.

## Review checklist before handoff

Correct long-term references; ADRs filed or out of scope; dependency readiness; bounded
scope with non-goals; explicit ownership and invariants; migration/compat effects;
concurrency, cancellation, restart, failure semantics; security/authorization effects;
required tests and static guards; unambiguous closure criteria. If any is unanswerable,
the work is not ready.
