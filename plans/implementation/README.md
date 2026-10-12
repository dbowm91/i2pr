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

A **new corrective scope after an authoritative finalized blocked closure** receives a
new globally unique plan in the same subsystem, referencing that closure and enumerating
unclosed findings plus required regressions. **Routine defect remediation within an
active coherent pass** (wrong datadir, admission parser, retry after a verified source
fix, missing test evidence, incorrect log selector, etc.) stays **inside the same plan**:
fix, add a deterministic regression, and rerun its affected acceptance gate. An attempt
budget prevents identical no-progress retries, not rooted corrective development.

Every external-router handoff MUST identify a legitimate reference-owned trigger,
source-derived selection prerequisites, deterministic offline fixture checks, the
first failure-owner boundary, the narrow success oracle and which evidence is only
local/control versus true independent interop. A socket-only or idle-listener fixture
cannot by itself establish a triggered transport test; a randomized peer selector
cannot guarantee one named candidate unless its actual admission and selection are
verified. Separate cryptography, I2NP transport, NetDB, tunnel and application gates.
See [planning execution-loop policy](../README.md#bounded-execution-and-causal-corrective-policy).
