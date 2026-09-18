# Milestone Implementation Plans

Bounded plans handed directly to implementation agents, grouped by subsystem.

## Layout and naming

```text
implementation/<subsystem>/NNN-short-title.md
```

**Global i2pr plan numbers (`NNN`) are preserved**, not renumbered per subsystem — status
tokens, supersession chains, checkers, and doc cross-references are keyed on them.

## Required implementation-plan template

New plans MUST state: objective (one bounded outcome), why ready (closed hard deps, stable
interface deps), current implementation evidence, invariants that must not regress, scope
(in / explicitly out), required production changes, ordered work packages, failure /
cancellation / restart / contention semantics, compatibility and migration, required tests,
exact verification commands, documentation updates, acceptance criteria, stop conditions,
closure evidence required, and handoff notes.

## Handoff rules

Before assigning a plan to an agent: confirm the repository baseline, confirm hard
dependencies are closed, confirm unresolved decisions have ADRs or are out of scope, ensure
one-coherent-pass sizing, and register the plan in `plans/registry.md`.

Corrective work receives a new plan in the same subsystem directory referencing the
original plan and closure record, enumerating unclosed findings with regression evidence.
