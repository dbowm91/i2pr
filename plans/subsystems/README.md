# Subsystem Roadmaps

Subsystem roadmaps translate i2pr's canonical direction (`GUARDRAILS.md`, `specs/`,
`docs/adr/`) into coherent, dependency-aware workstreams. They are not coding-agent
checklists — commit-specific mechanics belong in `plans/implementation/`.

## Naming

```text
<subsystem>-roadmap.md
```

Subsystem names are stable kebab-case. Do not encode dates in filenames.

## Required roadmap structure

```markdown
# <Subsystem> Roadmap

Status: active | closed | superseded

Long-term references:
- `GUARDRAILS.md`, `specs/CONFORMANCE.md`, `specs/support.toml`

Related ADRs:
- `docs/adr/NNNN-...md`

## 1. Purpose and ownership boundary
## 2. Work classification (invariant / capability / infrastructure / polish)
## 3. Non-goals
## 4. Current state (authority plan + token)
## 5. Target architecture
## 6. Dependency graph (plan sequence)
## 7. Milestones (one row per historic plan: state, i2pr token, implementation, closure)
## 8. Cross-cutting requirements
## 9. Verification strategy (evidence lanes)
## 10. Risks and decision points
## 11. Completion definition
## 12. Milestone status summary
```

## Roadmap rules

- Link to canonical requirements rather than duplicating them.
- Define ownership boundaries before milestones; distinguish infrastructure from capability.
- Preserve completed milestone history; state non-goals to prevent scope expansion.
- Keep **global i2pr plan numbers** in every filename and table (see `plans/README.md`).
- The i2pr status token in the closure record is authoritative; the `state` column is only
  the codegg-registry projection.
