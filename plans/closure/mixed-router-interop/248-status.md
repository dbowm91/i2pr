# Plan 248 status — interoperability evidence-policy reconciliation and M11 handoff

Status: **passed-interop-evidence-policy-reconciliation-and-m11-handoff**

Plan of record:
plans/implementation/mixed-router-interop/248-interop-evidence-policy-reconciliation-and-m11-handoff.md

ADR authority:
docs/adr/0026-staged-interoperability-progression-and-java-debt.md

## Disposition

This is a planning/canonical authority transition, not a runtime capability pass.

- M6 experimental mixed-router progression: passed via exact-pinned i2pd Plan 193.
- M6 Java full-router compatibility: retained/deferred at Plan 247, nonblocking.
- M6 full two-family router conformance: not claimed.
- M10 product authority: closed via Plans 214/215.
- Plan 204 convergence gate: superseded by Plan 248, not retroactively executed.
- Next dependency-ready product plan: Plan 249, M11 transit admission foundation.

## Evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Preserve i2pd authority | Plan 193 unchanged | passed |
| Preserve Java history | Plans 201 and 217-247 retained; current status amended only | passed |
| Do not invent Java pass | ADR 0026 + registry retain Java debt | passed |
| Keep M10 authority | Plan 215 remains product authority | passed |
| Keep strong full-conformance bar | specs/CONFORMANCE.md retains two independent families for full router-to-router claims | passed |
| Resume router development | M11 roadmap + Plan 249 registered | passed |
| No production/test code change | tree changes are planning/spec/docs only | passed |

## Verification

No Rust/runtime test suite was used as Plan 248 acceptance evidence because no production,
test, harness, build, or reference-pin code changes.

Review covered current main registry/roadmaps, Plan 193 evidence, Plan 201/247 Java
authority, Plan 215/204 M10 authority, specs/CONFORMANCE.md, existing short-build /
participant / router-I2NP seams, current official I2P tunnel/I2NP specifications, and
exact-pinned i2pd/Java transit behavior.

## Security and compatibility

No externally observable protocol behavior, advertisement, public-network enablement,
dependency, or pin changes. Java compatibility evidence is retained.

## Known limitations

Full Java-router reverse Streaming remains open. Plan 247 live polling integration remains
unexecuted. NTCP2 remains under its own experimental/non-advertised authority. M11/M12 are
not implemented by this plan.

## Unblock audit

Plan 201 becomes nonblocking retained debt. Plan 204 no longer blocks M10/M11. Stable
short-build crypto/roles, authenticated router-I2NP dispatch, controlled SSU2/i2pd
infrastructure, and closed earlier product layers make the M11 foundation
dependency-ready.

Plan 249 is registered ready. No M12 implementation plan is registered; M12 remains gated
on M11 controlled transit/resource closure.
